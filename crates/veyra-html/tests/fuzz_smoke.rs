// Deterministic fuzz smoke for the HTML parser (roadmap Phase 2,
// acceptance criterion 3: "Fuzz target runs >= a fixed CI budget with zero
// crashes/hangs").
//
// CI budget: 20,000 mutated inputs per seed set, xorshift-seeded (no
// wall-clock, no threads, no external corpus). The assertion is the
// browser-engine contract: the parser never panics and always returns
// (typed error or tree) — a hang fails the test's own runtime visibly in
// CI. The cargo-fuzz harness for libFuzzer-based runs ships separately
// (fuzz/); this smoke runs in every CI push without nightly toolchains.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_html::parse_document;
use veyra_url::Url;

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
    "<!DOCTYPE html><html><head><title>t</title></head><body><p class=\"a\">x</p></body></html>",
    "<div><template><tr><td>cell</td></tr></template><table><tr></table>",
    "<svg><foreignObject><math><annotation-xml encoding=\"text/html\"><div>",
    "<select><option><script>var a = \"</select>\";</script>",
    "<a href=\"x\"><a href=\"y\"><b>adopt</b></a></a>",
    "<p a=\"1\" a=\"2\" a=3 a><b><table><i>x<td>y",
    "<![CDATA[ cdata ]]><!-- comment --><!doctype html PUBLIC \"p\" \"s\">",
    "<x a=>>&lt;&amp;&#65;&#x41;&notarealentity;&",
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
                const INTERESTING: &[u8] = b"<>\"'/&=\x00\t\n\\{}[]";
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
                // Duplicate a slice (structure amplification).
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
                // Splice in a tag-ish fragment.
                const FRAGMENTS: &[&str] = &[
                    "<template>",
                    "</template>",
                    "<table>",
                    "<td>",
                    "<svg>",
                    "<noscript>",
                    "<option>",
                    "<!doctype x>",
                    "<?pi?>",
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
    let origin = Url::parse("https://fuzz.example/").unwrap().origin();
    let mut rng = Xorshift(0x5EED_2026_1015);
    let mut parsed = 0usize;
    let mut rejected = 0usize;

    for seed in SEEDS {
        for _ in 0..2_500 {
            let input = mutate(&mut rng, seed.as_bytes());
            match parse_document(&input, origin.clone()) {
                Ok(_) => parsed += 1,
                Err(_) => rejected += 1,
            }
        }
    }

    // 8 seeds x 2500 = 20,000 inputs: every one either parsed or was
    // rejected with a typed error. A panic or hang would fail this test.
    assert_eq!(parsed + rejected, 20_000);
    println!("fuzz smoke: {parsed} parsed, {rejected} rejected, 0 crashes");
}
