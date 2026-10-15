//! Phase 1 benchmark suite — see `docs/benchmarks/baseline-2026-phase1.md`.
//!
//! Measured operations (all in-process, deterministic inputs):
//! 1. `scene_render` — the diagnostic scene rendered on `CpuRaster`
//!    (the CPU raster path ADR-0005 designates as the CI determinism anchor).
//! 2. `clear` — full-buffer fill only (lower bound of the raster path).
//! 3. `canonical_hash` — canonical serialization + SHA-256 of the rendered
//!    buffer (the golden-image identity op).
//! 4. `png_encode` — PNG encoding of the rendered buffer (the screenshot
//!    harness's file step).
//! 5. `config_parse` — TOML config parsing (shell startup path).
//! 6. `screenshot_e2e` — render + encode + hash: the full headless
//!    screenshot pipeline minus filesystem I/O.
//!
//! Viewports: 800x600 (the golden/CI viewport) and 1920x1080 (scaling
//! reference). Criterion runs on the `bench` profile (inherits the
//! release profile: thin LTO, 1 codegen unit) — recorded in the baseline
//! note's methodology section.

// Scoped lint exception, documented per engineering standards §2: the
// denied `unwrap`/`expect`/`panic` lints target *untrusted input* handling
// in production paths. Test/bench fixtures are provably internal, and
// tests fail through the standard assert machinery. The workspace lint
// configuration is unchanged; non-test code keeps the full deny set.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use veyra_gfx::{CpuRaster, RasterSurface, Rgba8, fixture::render_diagnostic_scene};
use veyra_shell::config::Config;
use veyra_shell::screenshot;

const VIEWPORTS: [(u32, u32); 2] = [(800, 600), (1920, 1080)];

fn rendered(width: u32, height: u32) -> CpuRaster {
    let mut raster = CpuRaster::new(width, height).expect("benchmark viewport is valid");
    render_diagnostic_scene(&mut raster).expect("scene renders on valid viewport");
    raster
}

fn bench_scene_render(c: &mut Criterion) {
    let mut group = c.benchmark_group("scene_render");
    for (w, h) in VIEWPORTS {
        group.bench_function(format!("{w}x{h}"), |b| {
            b.iter_batched(
                || CpuRaster::new(w, h).expect("benchmark viewport is valid"),
                |mut raster| {
                    render_diagnostic_scene(black_box(&mut raster))
                        .expect("scene renders on valid viewport");
                    black_box(raster.buffer().data().len());
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_clear(c: &mut Criterion) {
    let mut group = c.benchmark_group("clear");
    for (w, h) in VIEWPORTS {
        group.bench_function(format!("{w}x{h}"), |b| {
            b.iter_batched(
                || rendered(w, h),
                |mut raster| {
                    raster.clear(black_box(Rgba8::opaque(0x20, 0x20, 0x28)));
                    black_box(raster.buffer().data().len());
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_canonical_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("canonical_hash");
    for (w, h) in VIEWPORTS {
        let raster = rendered(w, h);
        group.bench_function(format!("{w}x{h}"), |b| {
            b.iter(|| {
                let bytes = raster.buffer().canonical_bytes();
                black_box(veyra_shell::hashing::sha256_hex(black_box(&bytes)));
            });
        });
    }
    group.finish();
}

fn bench_png_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("png_encode");
    for (w, h) in VIEWPORTS {
        let raster = rendered(w, h);
        group.bench_function(format!("{w}x{h}"), |b| {
            b.iter(|| {
                let encoded =
                    screenshot::encode_png(raster.buffer()).expect("encoding a valid buffer");
                black_box(encoded.len());
            });
        });
    }
    group.finish();
}

fn bench_config_parse(c: &mut Criterion) {
    let mut c = c.benchmark_group("config_parse");
    c.bench_function("default_document", |b| {
        let text = std::include_str!("../tests/fixtures/sample-config.toml");
        b.iter(|| {
            let config = Config::parse_document(black_box(text)).expect("fixture config is valid");
            black_box(config.window.width);
        });
    });
    c.finish();
}

fn bench_screenshot_e2e(c: &mut Criterion) {
    let mut group = c.benchmark_group("screenshot_e2e");
    for (w, h) in VIEWPORTS {
        group.bench_function(format!("{w}x{h}"), |b| {
            b.iter_batched(
                || CpuRaster::new(w, h).expect("benchmark viewport is valid"),
                |mut raster| {
                    render_diagnostic_scene(&mut raster).expect("scene renders");
                    let encoded =
                        screenshot::encode_png(raster.buffer()).expect("encoding a valid buffer");
                    let bytes = raster.buffer().canonical_bytes();
                    black_box(veyra_shell::hashing::sha256_hex(black_box(&bytes)));
                    black_box(encoded.len());
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_scene_render,
    bench_clear,
    bench_canonical_hash,
    bench_png_encode,
    bench_config_parse,
    bench_screenshot_e2e
);
criterion_main!(benches);
