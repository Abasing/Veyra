//! Golden-image tests: the reviewed fixture is the truth; the render must
//! match it at the pixel level and by canonical hash.
//!
//! Regeneration procedure (deliberate, single entry point, never
//! automatic):
//!
//! ```sh
//! VEYRA_UPDATE_GOLDEN=1 cargo test -p veyra-shell --test golden \
//!     regenerates_fixtures -- --exact --ignored --nocapture
//! cargo test -p veyra-shell --test golden
//! ```
//!
//! The regeneration test is `#[ignore]`d so ordinary suite runs (CI
//! included) never touch the fixtures; it is a maintenance entry point,
//! invoked explicitly as shown, and prints the new canonical hash. It is
//! run with `--exact` so it executes alone (no parallel reader observes a
//! half-written fixture). Regenerated fixtures must be reviewed like
//! code: confirm the scene version was bumped in
//! `veyra_gfx::fixture::DIAGNOSTIC_SCENE_VERSION`, diff the PNG, and
//! update the compatibility-matrix evidence if the hash is referenced
//! there. The verification tests in this file never write.

// Scoped lint exception, documented per engineering standards §2: the
// denied `unwrap`/`expect`/`panic` lints target *untrusted input* handling
// in production paths. Test fixtures are provably internal, and tests fail
// through the standard assert machinery. The workspace lint configuration
// is unchanged; non-test code keeps the full deny set.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use veyra_gfx::RasterSurface as _;
use veyra_shell::golden;
use veyra_shell::hashing::sha256_hex;
use veyra_shell::screenshot;

/// Explicit regeneration entry point — see the module docs for the exact
/// procedure. `#[ignore]`d so ordinary suite runs never write; fails
/// without `VEYRA_UPDATE_GOLDEN=1` so it can never silently do nothing.
#[test]
#[ignore = "maintenance entry point: regenerates fixtures, run explicitly with VEYRA_UPDATE_GOLDEN=1"]
fn regenerates_fixtures() {
    if !std::env::var(golden::UPDATE_ENV).is_ok_and(|v| v == "1") {
        panic!(
            "fixture regeneration is explicit: run with {}=1 (and -- --exact) — \
             see the module docs in tests/golden.rs",
            golden::UPDATE_ENV
        );
    }
    eprintln!(
        "NOTICE: regenerating golden fixtures at {}; review the diff before \
         committing (the scene version must be bumped for content changes).",
        golden::golden_dir().display()
    );
    let hash = golden::write_fixture_into(
        &golden::golden_dir(),
        golden::GOLDEN_WIDTH,
        golden::GOLDEN_HEIGHT,
    )
    .expect("fixture regeneration");
    eprintln!("NOTICE: regenerated fixtures with canonical hash {hash}");
    assert_eq!(hash.len(), 64, "regeneration must report a full hash");
}

/// The canonical golden fixture check: hash + decoded-pixel equality.
#[test]
fn golden_fixture_matches_rendered_scene() {
    let report = golden::verify_against_fixture(golden::GOLDEN_WIDTH, golden::GOLDEN_HEIGHT)
        .expect("golden fixture must match the rendered scene");
    assert_eq!(report.rendered_hash, report.fixture_hash);
    assert_eq!(report.rendered_hash.len(), 64);
}

/// Drift check: a fresh render + fresh encode must reproduce the on-disk
/// fixture bytes exactly. Catches scene or encoder drift that a hash
/// comparison alone could miss. Never writes.
#[test]
fn fixture_is_current_with_the_scene() {
    let report = golden::fixture_is_current(golden::GOLDEN_WIDTH, golden::GOLDEN_HEIGHT)
        .expect("fixture must be current");
    assert!(!report.fixture_hash.is_empty());
}

/// Sensitivity check: a different viewport must produce a different
/// canonical hash (guards against a hash that ignores content).
#[test]
fn different_viewport_produces_different_hash() {
    let golden_report = golden::verify_against_fixture(golden::GOLDEN_WIDTH, golden::GOLDEN_HEIGHT)
        .expect("golden fixture");
    let other = screenshot::render_to_buffer(640, 480, veyra_shell::config::SceneKind::Diagnostic)
        .expect("render");
    let other_hash = sha256_hex(&other.buffer().canonical_bytes());
    assert_ne!(
        golden_report.rendered_hash, other_hash,
        "different viewports must not share a canonical hash"
    );
}

/// Regeneration into a fresh directory must produce byte-identical files
/// to the reviewed fixtures (proves the committed fixtures are exactly
/// what the documented procedure produces).
#[test]
fn regeneration_reproduces_committed_fixtures() {
    let dir = std::env::temp_dir().join(format!("veyra-golden-regen-{}", std::process::id()));
    let fresh_hash = golden::write_fixture_into(&dir, golden::GOLDEN_WIDTH, golden::GOLDEN_HEIGHT)
        .expect("regeneration");
    let committed_hash_text = std::fs::read_to_string(golden::fixture_hash_path(
        golden::GOLDEN_WIDTH,
        golden::GOLDEN_HEIGHT,
    ))
    .expect("committed fixture hash");
    let committed_hash = committed_hash_text
        .lines()
        .next()
        .unwrap_or_default()
        .trim();
    assert_eq!(fresh_hash, committed_hash);
    let fresh_png = std::fs::read(dir.join(format!(
        "{}.png",
        golden::fixture_stem(golden::GOLDEN_WIDTH, golden::GOLDEN_HEIGHT)
    )))
    .expect("fresh png");
    let committed_png = std::fs::read(golden::fixture_png_path(
        golden::GOLDEN_WIDTH,
        golden::GOLDEN_HEIGHT,
    ))
    .expect("committed png");
    assert_eq!(fresh_png, committed_png, "PNG bytes must be reproducible");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Known landmark pixels from the fixture PNG are asserted directly — the
/// golden content is not merely "whatever the code produced last": these
/// values were reviewed when the fixture was committed (corner markers,
/// frame, center target, background).
#[test]
fn fixture_png_contains_reviewed_landmark_pixels() {
    let png = std::fs::read(golden::fixture_png_path(
        golden::GOLDEN_WIDTH,
        golden::GOLDEN_HEIGHT,
    ))
    .expect("fixture png");
    let pixels = golden::decode_png_pixels(&png, golden::GOLDEN_WIDTH, golden::GOLDEN_HEIGHT)
        .expect("fixture decodes");

    fn pixel_at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let base = ((y * width + x) * 4) as usize;
        [
            pixels[base],
            pixels[base + 1],
            pixels[base + 2],
            pixels[base + 3],
        ]
    }

    // Frame border (top edge, middle) is light gray.
    assert_eq!(
        pixel_at(&pixels, golden::GOLDEN_WIDTH, 400, 0),
        [0xE8, 0xE8, 0xE8, 0xFF]
    );
    // Top-left corner marker (frame is 8 px thick at 800 width).
    assert_eq!(
        pixel_at(&pixels, golden::GOLDEN_WIDTH, 9, 9),
        [0xE0, 0x30, 0x30, 0xFF]
    );
    // Center of the concentric target is the white core.
    assert_eq!(
        pixel_at(&pixels, golden::GOLDEN_WIDTH, 400, 300),
        [0xFF, 0xFF, 0xFF, 0xFF]
    );
    // Background slate below the checkerboard (383..592) and the center
    // target (which spans y 180..420), away from all four corner markers.
    assert_eq!(
        pixel_at(&pixels, golden::GOLDEN_WIDTH, 400, 570),
        [0x20, 0x20, 0x28, 0xFF]
    );
    // Bottom-left corner marker is blue (marker 25 px at x 8..33,
    // y 567..592).
    assert_eq!(
        pixel_at(&pixels, golden::GOLDEN_WIDTH, 10, 570),
        [0x30, 0x60, 0xE0, 0xFF]
    );
}
