# Research: Dependency and License Audit

status: Investigated (first pass) | last-updated: 2026-10-15 | maintainer: foundation work

## Evidence basis

All crate rows carry data retrieved live from the **crates.io API on
2026-10-15** `[E]`: newest version, license field from the newest version's
metadata, total downloads, and last-update date. GitHub repository rows come
from the GitHub API on the same date. Where a license string differs between
crates.io and the repository, both are shown and the conflict is noted.
**This audit records candidate status only — nothing here is an adopted
dependency.** The workspace currently has **zero external dependencies**;
every row below is a *candidate to be evaluated at its phase gate*.

## 1. Policy summary

1. **License allow-list** (enforced by `deny.toml`): Apache-2.0 (incl.
   LLVM-exception), MIT, ISC, BSD-2/3, Unicode-3.0, Zlib, 0BSD, CC0-1.0.
2. **MPL-2.0 (file-level copyleft)** — *not* in the allow-list today. The
   Servo family (stylo, cssparser, selectors) and symphonia are MPL-2.0;
   adopting them requires an explicit ADR accepting file-level license
   isolation and NOTICE obligations. This is deliberate friction, not an
   oversight.
3. **GPL/LGPL/AGPL** — rejected for engine components.
4. **Language policy**: pure-Rust preferred; C/C++-core libraries (v8,
   skia-safe, rquickjs, rusqlite, ffmpeg bindings) are rejected for new
   subsystems per ADR-0001; OS-API *bindings* (windows-rs, nix, rustls-
   platform-verifier's use of OS verifiers) are acceptable as platform
   boundaries, not engine internals — see ADR-0007.
5. **Maturity signals** used below: downloads, last-update recency, 1.0
   status, organizational backing. Signals are indicators, not guarantees;
   each phase-gate adoption re-checks them.

## 2. Execution engines

| Crate | License `[E]` | Version | Downloads | Last update | Assessment |
|---|---|---|---|---|---|
| `boa_engine` | Unlicense OR MIT (crates.io) / MIT (repo) | 0.22.0 | 5.5M | 2026-08-28 | **Lead JS-engine candidate** (ADR-0002). Self-reported ">90% of latest ECMAScript" (README, 2026-10-15); Veyra re-measures via Test262. |
| `nova_vm` | MPL-2.0 | 1.0.0 | 1.8K | 2026-03-15 | Emerging engine; MPL license triggers policy rule 2. Watch-list, re-evaluate Phase 5. |
| `wasmtime` | Apache-2.0 WITH LLVM-exception | 49.0.3 | 40.0M | 2026-10-14 | Production WASM runtime (JIT/Cranelift). Phase 8+ JIT path (ADR-0009). |
| `wasmi` | MIT/Apache-2.0 | 2.0.0 | 25.0M | 2026-09-01 | Interpreter WASM runtime; **Phase 8 starting candidate** (small audit surface). |
| `wasmer` | MIT | 7.5.0 | 8.8M | 2026-10-01 | Alternative; not preferred (license fine; ecosystem overlap with wasmtime). |
| `rquickjs` | MIT | 0.14.0 | 5.3M | 2026-09-18 | Bindings to C QuickJS-ng — **rejected as runtime** (C core); reference oracle only. |
| `v8` (rusty_v8) | MIT | 152.2.0 | 31.3M | 2026-08-20 | Bindings to C++ V8 — **rejected**: violates the independence objective (ADR-0001). |

## 3. HTML / URL / encoding

| Crate | License `[E]` | Version | Downloads | Last update | Assessment |
|---|---|---|---|---|---|
| `html5ever` | MIT OR Apache-2.0 | 0.40.1 | 124.7M | 2026-09-14 | Spec HTML5 parsing; **Phase 2 candidate** (ADR-0003 records adopt-vs-reimplement). |
| `html5gum` | MIT | 0.8.4 | 1.8M | 2026-06-22 | Lighter parser; fallback option. |
| `url` | MIT OR Apache-2.0 | 2.5.8 | 954.7M | 2026-01-05 | WHATWG URL — adopt as the sole URL authority (`veyra-url` wrapper), Phase 2. |
| `idna` / `percent-encoding` | MIT OR Apache-2.0 | 1.1.0 / 2.3.2 | 1.04B / 982.4M | 2025-08-21 | URL companions (pulled by `url`). |
| `unicode-segmentation` | MIT OR Apache-2.0 | 1.13.3 | 608.1M | 2026-06-01 | Text segmentation (UAX#29). |
| `unicode-bidi` | MIT OR Apache-2.0 | 0.3.18 | 536.6M | 2024-12-16 | UAX#9 bidi — Phase 3/7. |

## 4. CSS / layout / text

| Crate | License `[E]` | Version | Downloads | Last update | Assessment |
|---|---|---|---|---|---|
| `cssparser` | MPL-2.0 | 0.38.0 | 101.7M | 2026-09-07 | MPL — gate on ADR-0003. Tokenizer/parser core. |
| `selectors` | MPL-2.0 | 0.41.0 | 86.2M | 2026-09-30 | MPL — gate on ADR-0003. Selector matching. |
| `stylo` | MPL-2.0 | 0.22.0 | 405.4K | 2026-09-30 | Extracted Gecko style engine; **highest-leverage / highest-risk** option; ADR-0003 spike required. |
| `taffy` | MIT | 0.15.0 | 16.3M | 2026-10-12 | Flexbox + Grid; **Phase 3 candidate**. Block/inline layout not covered — custom work. |
| `parley` / `fontique` | Apache-2.0 OR MIT | 0.12.0 | 4.1M | 2026-10-09 | Linebender text stack (shaping orchestration, fallback, font enumeration) — Phase 3. |
| `rustybuzz` | MIT | 0.20.1 | 34.7M | 2024-11-12 | HarfBuzz-shaped shaping in Rust — Phase 3. (Update cadence slow = stable.) |
| `ttf-parser` | MIT OR Apache-2.0 | 0.25.1 | 118.6M | 2024-11-29 | SFNT parsing. |
| `swash` | Apache-2.0 OR MIT | 0.2.10 | 15.6M | 2026-07-17 | Text scaling/rasterization. |
| `fontdb` | MIT | 0.24.0 | 38.3M | 2026-07-29 | System font discovery. |
| `cosmic-text` | MIT OR Apache-2.0 | 0.19.0 | 10.3M | 2026-04-22 | Alternative text stack; evaluated against parley at Phase 3. |
| `icu` (ICU4X) | Unicode-3.0 | 2.3.1 | 2.5M | 2026-08-20 | i18n primitives (Intl API path), Unicode-3.0 allow-listed. |

## 5. Graphics

| Crate | License `[E]` | Version | Downloads | Last update | Assessment |
|---|---|---|---|---|---|
| `wgpu` | MIT OR Apache-2.0 | 30.0.1 | 40.9M | 2026-08-22 | GPU abstraction (Vulkan/Metal/DX12/GL) — **ADR-0005 candidate**. Talks to native drivers via OS APIs (policy: acceptable platform boundary). |
| `winit` | Apache-2.0 | 0.31.0-beta.3 | 58.2M | 2026-09-04 | Windowing; **beta noted** — pin exact version at Phase 1. |
| `vello` | Apache-2.0 OR MIT | 0.11.0 | 1.0M | 2026-10-02 | GPU 2D vector rendering; pre-1.0 — evaluate at Phase 7. |
| `tiny-skia` | BSD-3-Clause (crates.io; repo historically MIT OR Apache-2.0 — **license conflict to re-verify at adoption**) | 0.12.0 | 55.8M | 2026-02-02 | CPU rasterizer — Phase 3 paint path candidate. |
| `resvg` / `usvg` | Apache-2.0 OR MIT | 0.48.1 | 30.4M | 2026-08-02 | SVG rendering — Phase 7. |
| `image` | MIT OR Apache-2.0 | 0.25.10 | 214.2M | 2026-03-10 | Image decode (PNG/JPEG/GIF/WebP). AVIF remains a **gap** (no mature pure-Rust decoder identified). |
| `glyphon` | MIT OR Apache-2.0 OR Zlib | 0.12.0 | 1.5M | 2026-07-09 | GPU text rendering atop vello — Phase 7 evaluation. |
| `skia-safe` | MIT | 0.153.3 | 4.5M | 2026-09-04 | C++ Skia bindings — **rejected** (C++ core). |

## 6. Networking

| Crate | License `[E]` | Version | Downloads | Last update | Assessment |
|---|---|---|---|---|---|
| `hyper` | MIT | 1.12.0 | 1.01B | 2026-10-06 | HTTP/1.1 + H2 client core — Phase 2. |
| `h2` | MIT | 0.4.20 | 884.0M | 2026-10-06 | HTTP/2. |
| `rustls` | Apache-2.0 OR ISC OR MIT | 0.23.46 | 1.00B | 2026-10-14 | TLS — memory-safe stack. |
| `rustls-platform-verifier` | MIT OR Apache-2.0 | 0.7.1 | 177.3M | 2026-09-24 | OS trust-store verification (platform boundary per ADR-0007). |
| `webpki-roots` | CDLA-Permissive-2.0 | 1.0.9 | 821.6M | 2026-07-18 | Bundled Mozilla CA trust roots — Phase 2 (ADR-0013 deviation from platform-verifier, revisit Phase 6). |
| `ring` | Apache-2.0 AND ISC | 0.17.14 | 806.9M | 2025-03-11 | rustls crypto provider (transitive, ADR-0008 stack); publishes `Apache-2.0 AND ISC` — no OpenSSL clause in the crates.io expression. |
| `quinn` | MIT OR Apache-2.0 | 0.11.12 | 354.3M | 2026-09-14 | QUIC — Phase 7–8. |
| `h3` | MIT | 0.0.8 | 4.9M | 2025-05-06 | **0.0.x, update >1y — maturity risk**; H3 deferred (ADR-0008). |
| `tokio` | MIT | 1.53.3 | 1.07B | 2026-10-13 | Async runtime. |
| `hickory-resolver` / `hickory-proto` | MIT OR Apache-2.0 | 0.26.3 | 87.8M / 91.0M | 2026-09-10 | DNS(+DoH path). |
| `interprocess` | 0BSD OR Apache-2.0 (crates.io; repo also shows MIT — **conflict to re-verify**) | 2.4.5 | 16.0M | 2026-10-10 | Local IPC — Phase 6. |
| `tungstenite`-class WS | (to be picked) | — | — | — | WebSocket — Phase 8; selection deferred. |

## 7. Storage / OS / platform services

| Crate | License `[E]` | Version | Downloads | Last update | Assessment |
|---|---|---|---|---|---|
| `redb` | MIT OR Apache-2.0 | 4.4.0 | 13.4M | 2026-10-13 | Embedded ACID store — **ADR-0010 choice** over SQLite (C) and sled (alpha). |
| `fjall` | MIT OR Apache-2.0 | 3.1.12 | 1.8M | 2026-10-03 | LSM alternative for cache-like stores; evaluate later. |
| `sled` | MIT OR Apache-2.0 | 1.0.0-alpha.124 | 17.0M | 2024-10-11 | **Not adopted** — unreleased 1.0, stale. |
| `rusqlite` | MIT | 0.40.2 | 125.7M | 2026-08-08 | C SQLite — **rejected for engine stores** per language policy; revisit only via ADR-0007 exception. |
| `nix` | MIT | 0.31.3 | 874.1M | 2026-05-11 | Unix API bindings (namespaces, fds). |
| `landlock` | MIT OR Apache-2.0 | 0.4.7 | 18.4M | 2026-07-27 | Linux LSM sandboxing — Phase 6. |
| `seccompiler` | Apache-2.0 OR BSD-3-Clause | 0.5.0 | 24.3M | 2025-03-07 | seccomp filter generation — Phase 6. |
| `capsicum` | MPL-2.0 | 0.4.5 | 21.3K | 2026-06-23 | FreeBSD capability mode — future port work only. |
| `accesskit` | MIT OR Apache-2.0 | 0.25.1 | 29.0M | 2026-09-25 | A11y tree + platform adapters — Phase 10. |
| `keyring` | MIT OR Apache-2.0 | 4.2.0 | 31.4M | 2026-08-29 | OS credential storage — Phase 10. |
| `symphonia` | MPL-2.0 | 0.6.1 | 17.2M | 2026-08-13 | Audio decode — MPL gate + Phase 9. |
| `tracing` | MIT | 0.1.44 | 933.3M | 2025-12-18 | Structured diagnostics. |
| `criterion` | Apache-2.0 OR MIT | 0.8.2 | 307.1M | 2026-02-04 | Benchmarking. |

## 8. Gaps in the Rust ecosystem (recorded honestly)

| Need | Status |
|---|---|
| Video decode (H.264/VP9/AV1) | No mature pure-Rust decoder identified; C decoders (dav1d/openh264/ffmpeg) rejected by policy → **media gap**, Phase 9 decision required (ADR-0007 exception vs. feature gap) |
| AVIF image decode | Same situation as video decode |
| PDF writer (printing) | No browser-grade pure-Rust PDF writer identified; custom work planned (Phase 10) |
| PAC (proxy auto-config) evaluator | None mature; custom implementation or feature gap (Phase 9) |
| DRM/EME | Proprietary CDM ecosystem; **out of scope** (documented limitation) |
| Inline text layout (CSS Text full) | No complete crate; parley-class stacks get close; custom work expected (Phase 7) |
| Spec-grade WebRTC | `webrtc-rs` exists but real-world hardening unverified — Phase 9 evaluation with fallback "feature gap" |
| GPU video compositing / HDR color | No pure-Rust stack identified — Phase 10+ |

## 9. Reference projects studied (not dependencies)

- **Servo** [E: MPL-2.0, 38,152 stars, active]: architecture reference for
  Rust engine decomposition; components above are its extractable crates.
- **quickjs-ng** [E: MIT, active]: C engine — semantics reference only.
- **WPT [T1], Test262 [T3], html5lib-tests [E: MIT]**: test corpora with
  license/attribution obligations (WPT dual license requires vendored
  license texts — see compatibility program).

## 10. Open questions

1. **[U] `tiny-skia` and `interprocess` license-field conflicts** (crates.io
   vs repo history) — resolve at adoption time by reading the vendored
   license files; note `tiny-skia` workspace policy impact.
2. **[U] Boa dual-license presentation** (crates.io "Unlicense OR MIT" vs
   repo MIT) — clarify before ADR-0002 finalization.
3. **[U] `h3` trajectory** (0.0.8, stale >1y) — revisit Phase 7; H3 is
   perf-motivated only.
4. **[U] AV1/video** — monitor pure-Rust decoder projects; escalate to
   ADR-0007 exception discussion only if a browser-viable path appears.
5. **[U] `winit` 0.31 beta** — pin stable release at Phase 1.

## Confidence and gaps

- High: the table data itself (retrieved live from crates.io/GitHub APIs
  on 2026-10-15; reproducible via the archived pull).
- Medium: maturity interpretations (download counts and activity are
  indicators, not guarantees); license-field conflicts noted inline.
- Low/pending: adoption decisions — every row is a candidate until its
  phase gate; nothing here authorizes a dependency by itself.
