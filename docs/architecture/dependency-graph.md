# Veyra Dependency Graph

status: Updated (Phase 3) | last-updated: 2026-10-17 | companion-to: system-overview.md

This document defines the **allowed dependency directions** between Veyra
crates and the invariants that keep them honest. It is the contract CI and
review enforce as crates appear; the graph below is the target state at
Phase 8. Current tree (Phase 3): `veyra-doctor` (tooling), `veyra-gfx`
(CPU raster), `veyra-shell` (application root; owns the navigation/session
browser layer), `veyra-url` (parsing authority), `veyra-dom` (arena tree),
`veyra-html` (HTML parsing), `veyra-net` (network service), `veyra-style`
(CSS parsing/cascade), `veyra-layout` (block/inline layout),
`veyra-text` (font collection, shaping, bidi — ADR-0015; the `LAYOUT -->
TEXT` edge is realized: layout's `ShapedMetrics` measures through the
shared `FontLibrary`, with `FixedMetrics` as the deterministic fallback
and golden-fixture provider), and `veyra-paint` (display lists — the
`PAINT --> LAYOUT` edge; fragments carry their paint-affecting computed
values so no style edge exists; rasterization via `PAINT --> GFX` is the
next paint increment).

## 1. Engine-crate dependency rules

```mermaid
flowchart TD
    DOCTOR["veyra-doctor (tools)"]

    subgraph shell_layer
        SHELL["veyra-shell"]
    end
    subgraph browser_layer
        NAV["navigation/session (module in shell→browser)"]
        PERM["permissions"]
        PROFILE["veyra-profile"]
    end
    subgraph services
        NET["veyra-net"]
        STORAGE["veyra-storage"]
        IPC["veyra-ipc"]
        SANDBOX["veyra-sandbox"]
        WASM["veyra-wasm"]
        WORKERS["veyra-workers"]
        DEVTOOLS["veyra-devtools"]
    end
    subgraph content_layer
        HTML["veyra-html"]
        DOM["veyra-dom"]
        STYLE["veyra-style"]
        LAYOUT["veyra-layout"]
        TEXT["veyra-text"]
        PAINT["veyra-paint"]
        GFX["veyra-gfx"]
        SCRIPT["veyra-script"]
        WEBID["veyra-webid"]
        EXEC["veyra-executor"]
        SERIAL["veyra-serialize"]
    end
    URL["veyra-url"]

    SHELL --> NAV --> NET
    SHELL --> GFX
    NAV --> URL
    NAV --> HTML
    HTML --> DOM
    DOM --> URL
    WEBID --> DOM
    WEBID --> SCRIPT
    SCRIPT --> EXEC
    WORKERS --> EXEC
    WORKERS --> SCRIPT
    DOM --> STYLE
    STYLE --> TEXT
    STYLE --> URL
    LAYOUT --> STYLE
    LAYOUT --> TEXT
    PAINT --> LAYOUT
    PAINT --> GFX
    NAV --> STORAGE
    PROFILE --> STORAGE
    WASM --> SCRIPT
    NET --> URL
    DEVTOOLS --> DOM
    DEVTOOLS --> SCRIPT
    SANDBOX --> IPC
```

Invariants (checked in review; a `veyra-doctor` extension will check them
mechanically once `Cargo.toml` edges exist):

1. **No cycles.** The content layer is a DAG ending in `veyra-paint`.
2. **`veyra-dom` depends on nothing in the engine** except `veyra-url`
   (origins live on nodes) — it is the system's stable core.
3. **`veyra-script`/`veyra-webid` are the only crates that see JS engine
   types.** Web API implementations elsewhere are engine-agnostic.
4. **`veyra-net` and `veyra-storage` never import content-layer crates.**
   They are trusted-service side; content reaches them only via
   capability-passing interfaces (security model §3).
5. **`veyra-url` is the only URL parser.** Everything else calls it
   (mirrors the Chromium centralized-URL finding, dossier networking §2).
6. **Untrusted-input leaf rule:** `veyra-html`, CSS parsing (in
   `veyra-style`), decoders, and IPC message decoding are leaf-ish crates
   with no privileged imports — they are the fuzz targets.
7. **`veyra-shell` is a root** (nothing depends on it). In Phase 1 the
   shell consumes `veyra-gfx` through the [`RasterSurface`] trait only —
   the same seam the Phase 7 GPU backend plugs into (ADR-0005).

[`RasterSurface`]: https://docs.rs/veyra-gfx

## 2. External dependencies (target state)

Consolidated from the dependency audit; each row's adoption is phase-gated:

| Area | Crates | Notes |
|---|---|---|
| URL | url, idna | adopted Phase 2 |
| Async/runtime | tokio | services only |
| HTTP/TLS/DNS | hyper, h2, rustls, rustls-platform-verifier, hickory-resolver | Phase 2 |
| HTML | html5ever | Phase 2 (ADR-0003) |
| Text | harfrust, read-fonts, unicode-bidi, unicode-segmentation (adopted Phase 3, ADR-0015 as superseded in part by ADR-0016 — the RUSTSEC-2026-0192 migration; parley/fontique deferred with recorded rationale) | shaping types never leave `veyra-text` (§5) |
| CSS/layout | cssparser/selectors (MPL gate) or stylo; taffy | Phase 3 (ADR-0003) |
| Graphics | tiny-skia; wgpu; (vello eval) | Phase 3/7 (ADR-0005) |
| SVG/images | resvg/usvg, image | Phase 7 |
| JS | boa_engine | Phase 4 (ADR-0002) |
| WASM | wasmi; wasmtime later | Phase 8 (ADR-0009) |
| Storage | redb | Phase 6 (ADR-0010) |
| IPC/sandbox | interprocess, nix, landlock, seccompiler | Phase 6 |
| A11y/OS | accesskit, keyring | Phase 10 |
| Tooling | tracing, criterion | ongoing |

Build-time policy: no build.rs network access; no proc-macro dependencies
in the content layer until the Phase 5 binding-generator decision.

## 3. Testing boundaries

- Unit tests live in-crate; integration tests per boundary in `tests/` of
  the owning crate; cross-crate determinism tests in `veyra-shell`'s
  harness (screenshot/display-list).
- Fuzz targets: `veyra-html` (bytes), `veyra-style` (CSS bytes),
  `veyra-url` (strings), `veyra-net` (response bytes), decoders (bytes),
  `veyra-ipc` (message bytes), `veyra-serialize` (value bytes).
- The dependency graph is what makes this tractable: leaf parsers cannot
  hide behavior behind service calls, so their tests are complete.

## 4. Phase-gate additions

Crates join this graph only at their roadmap phase, with the ADR that
justifies the boundary cited in the crate's `Cargo.toml` header comment.
The current tree contains `veyra-doctor`, `veyra-gfx` (Phase 1, brought
forward per roadmap — ADR-0005/0012), `veyra-shell` (Phase 1,
ADR-0007/0012; Phase 2 adds the browser layer), `veyra-url`,
`veyra-dom`, `veyra-html`, and `veyra-net` (Phase 2 — ADR-0008/0013),
`veyra-style` (Phase 3 — ADR-0003/0014: parsing consumes
cssparser/selectors at the crate boundary; the batch-2 cascade activates
the `DOM --> STYLE` and `STYLE --> URL` edges — matching adapts
`veyra-dom` elements to the `selectors` matcher, and `@import`/base-URL
resolution goes through `veyra-url`), `veyra-layout` (Phase 3 — ADR-0003
decision 3: custom block/inline; box-tree, BFC/IFC, intrinsic sizing),
and `veyra-text` (Phase 3 — ADR-0015/0016: harfrust/read-fonts/unicode-bidi
behind Veyra-owned types; the `LAYOUT --> TEXT` edge activates via
`veyra-layout`'s `ShapedMetrics` adapter), and `veyra-paint` (Phase 3 —
no ADR required: dependency-free crate per ADR-0003 decision 1's
display-list pipeline; the `PAINT --> LAYOUT` edge with paint data baked
into fragments); every other row in §1's
diagram is a target state, and
the review checklist rejects PRs that add crates without both the
roadmap phase and an ADR reference. When a boundary
turns out wrong in practice (cohesion signals: types imported solely to
satisfy visibility, cyclic test helpers, duplicated data structures),
the correction is an ADR amendment plus a re-draw of this document —
not a quiet merge of crates.

## 5. External-crate isolation rules

Every external dependency is consumed through a Veyra-owned wrapper
type or module at exactly one boundary, so that replacement remains a
local change:

| External surface | Isolating boundary | Rule |
|---|---|---|
| `url`, `idna` | `veyra-url` | no other crate may call URL parsing |
| `hyper`/`h2`/`rustls`/`quinn` | `veyra-net` internals | transport types never appear in public APIs |
| `html5ever` | `veyra-html` | emits `veyra-dom` nodes only |
| `boa_engine` | `veyra-script` | engine types never leave the facade (ADR-0002) |
| `wgpu`/`tiny-skia`/`vello` | `veyra-gfx` | device/queue types never leave the trait |
| `redb` | `veyra-storage` | database handles are crate-private |
| `parley`/`harfrust`/`swash` | `veyra-text` | shaped-run types are internal (currently: harfrust + read-fonts per ADR-0015/0016) |
| `tokio` | services only | engine core stays synchronous |

This is what converts "dependency risk" into "one-file swap": the audit
table's candidates can be re-evaluated without cascading changes.
