# Veyra Dependency Graph

status: Proposed (Phase 0) | last-updated: 2026-10-15 | companion-to: system-overview.md

This document defines the **allowed dependency directions** between Veyra
crates and the invariants that keep them honest. It is the contract CI and
review enforce as crates appear; the graph below is the target state at
Phase 8, not the current tree (currently: `veyra-doctor` only).

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
7. **`veyra-shell` is a root** (nothing depends on it).

## 2. External dependencies (target state)

Consolidated from the dependency audit; each row's adoption is phase-gated:

| Area | Crates | Notes |
|---|---|---|
| URL | url, idna | adopted Phase 2 |
| Async/runtime | tokio | services only |
| HTTP/TLS/DNS | hyper, h2, rustls, rustls-platform-verifier, hickory-resolver | Phase 2 |
| HTML | html5ever | Phase 2 (ADR-0003) |
| Text | parley, fontique, rustybuzz, ttf-parser, swash, fontdb, unicode-bidi, unicode-segmentation | Phase 3 |
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
