// Deterministic fuzz smoke for the CSS parser (roadmap Phase 3,
// acceptance criterion analog of Phase 2's: "fuzz target runs >= a fixed
// CI budget with zero crashes/hangs").
//
// CI budget: 20,000 mutated inputs per seed set, xorshift-seeded (no
// wall-clock, no threads, no external corpus). The assertion is the
// browser-engine contract: the parser never panics and always returns
// (typed error or stylesheet) — a hang fails the test's own runtime
// visibly in CI. The cargo-fuzz harness for libFuzzer-based runs ships
// separately (fuzz/); this smoke runs in every CI push without nightly
// toolchains.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_style::{Limits, Stylesheet};

struct Xorshift(u64);

impl Xorshift {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

const SEEDS: &[&str] = &[
    "h1 { color: crimson; !important } p:hover { margin: 0 auto }",
    "@media screen and (min-width: 100px) { div > a::before { content: \"x\" } @supports (display: grid) { b { flex: 1 } } }",
    "@import url(\"a.css\") layer(base) screen; @namespace svg url(http://www.w3.org/2000/svg);",
    "@font-face { font-family: \"Custom\"; src: url(f.woff2) format(\"woff2\"); unicode-range: U+0-7F; }",
    "@keyframes slide { from { left: 0 } 50% { left: 50% } to, 100% { left: 100% } }",
    "@layer base, components; @layer components { .btn { color: blue } }",
    "a:not(.x, #y):is(section, article):has(> img):nth-child(2n+1 of .z) { --custom: calc(1px + 2 * var(--w)) }",
    "[data-x~=\"v\"] svg|rect { fill: rgb(1 2 3 / 50%) } @unknown-at { weird: [structure; } trailing",
];

const MUTATORS: usize = 6;

fn mutate(rng: &mut Xorshift, input: &[u8]) -> Vec<u8> {
    let mut out = input.to_vec();
    let rounds = (rng.next_u64() % 4) + 1;
    for _ in 0..rounds {
        if out.is_empty() {
            break;
        }
        match rng.next_u64() as usize % MUTATORS {
            0 => {
                // Byte flip.
                let index = (rng.next_u64() as usize) % out.len();
                out[index] = (rng.next_u64() & 0xFF) as u8;
            }
            1 => {
                // Insert an interesting byte.
                const INTERESTING: &[u8] = b"<>\"'/&;{}()[]!@*.#\x00\t\n\\%-";
                let index = (rng.next_u64() as usize) % out.len();
                let byte = INTERESTING[(rng.next_u64() as usize) % INTERESTING.len()];
                out.insert(index, byte);
            }
            2 => {
                // Delete a byte.
                let index = (rng.next_u64() as usize) % out.len();
                out.remove(index);
            }
            3 => {
                // Duplicate a slice (structure amplification: nested
                // blocks, unbalanced delimiters).
                let start = (rng.next_u64() as usize) % out.len();
                let len =
                    ((rng.next_u64() as usize) % out.len().saturating_sub(start).max(1)).min(64);
                let slice = out[start..start + len].to_vec();
                let at = (rng.next_u64() as usize) % out.len();
                out.splice(at..at, slice);
            }
            4 => {
                // Truncate.
                let keep = (rng.next_u64() as usize) % out.len();
                out.truncate(keep);
            }
            _ => {
                // Splice in a CSS-shaped fragment.
                const FRAGMENTS: &[&str] = &[
                    "@media",
                    "{",
                    "}",
                    "@keyframes x { from {",
                    "calc(",
                    "url(",
                    "!important",
                    "/*",
                    "<!--",
                    "-->",
                    "\\",
                    "\"",
                    ":is(",
                    "@charset \"x\";",
                ];
                let at = (rng.next_u64() as usize) % out.len();
                let fragment = FRAGMENTS[(rng.next_u64() as usize) % FRAGMENTS.len()];
                out.splice(at..at, fragment.bytes());
            }
        }
    }
    out.truncate(64 * 1024); // bound memory; the parser's limits do the rest
    out
}

#[test]
fn fuzz_smoke_no_crashes_or_hangs() {
    let mut rng = Xorshift(0x5EED_2026_1016);
    let mut parsed = 0usize;
    let mut rejected = 0usize;
    for seed in SEEDS {
        let mut input = seed.as_bytes().to_vec();
        for _ in 0..(20_000 / SEEDS.len()) {
            input = mutate(&mut rng, &input);
            let result = Stylesheet::parse_bytes(&input, &Limits::default());
            match result {
                Ok(sheet) => {
                    // The round-trip property: canonical serialization
                    // must itself parse without panicking (it may
                    // differ in rule count — error recovery — but must
                    // be well-formed).
                    parsed += 1;
                    let text = sheet.to_css_string();
                    let _ = Stylesheet::parse(&text, &Limits::default());
                }
                Err(error) => {
                    // Typed errors only; the Display impl must not
                    // panic on attacker-derived shapes.
                    rejected += 1;
                    let _ = error.to_string();
                }
            }
        }
    }
    assert!(
        parsed > 0 && rejected > 0,
        "fuzz smoke must exercise both paths (parsed={parsed}, rejected={rejected})"
    );
}
