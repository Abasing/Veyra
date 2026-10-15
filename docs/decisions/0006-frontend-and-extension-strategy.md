# ADR 0006: Frontend integration and staged extension strategy

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

The charter names TypeScript the preferred language for Veyra's
application interface and frontend tooling. A browser's UI (tabs, omnibox,
settings) must be built somehow, and an extension story must be planned
honestly ([browser-product-features.md](../research/browser-product-features.md)
§2–3: MV3-shaped platform, isolated worlds, service workers, DNR; no
Chrome Web Store compatibility claims). The tension: a TypeScript browser
UI needs something to render it — either Veyra's own engine (self-hosting,
not viable early) or a native shell.

## Decision

1. **Phase 1: native diagnostic shell** — Rust (`winit`) window hosting
   the engine with a minimal diagnostic UI (URL bar, render surface,
   screenshot harness, logs). This is a *test rig*, not the product UI.
2. **Product chrome UI (Phase 9–10): TypeScript-based**, rendered by
   Veyra's own engine (self-hosted chrome) — the dogfooding path that
   keeps the charter's TypeScript preference honest. A native fallback
   (winit + egui-class) is pre-registered if self-hosting the chrome
   proves destabilizing; the fallback decision is evidence-based at
   Phase 9.
3. **Extensions: staged, subset-first.** Phase 9 implements a
   documented MV3-shaped subset (manifest, content scripts with isolated
   worlds, storage, tabs/messaging); each `chrome.*` namespace gets an
   explicit matrix entry (implemented / partial / not implemented).
   **No Chrome Web Store compatibility is claimed** — distribution will
   be Veyra's own; API compatibility is measured, not asserted.
4. **DevTools (Phase 9–10):** implement a CDP-shaped protocol subset
   over a JS-engine debug facade (hooks designed in Phase 4). Frontend
   reuse (Chromium's DevTools frontend) is a licensing question resolved
   before any code consideration; default plan is a minimal inspector UI.

## Alternatives considered

- **TypeScript UI from Phase 1** — rejected: requires the engine to
  render the UI; circular dependency on unimplemented engine.
- **Electron/Tauri-class webview shell for the UI** — rejected:
  embeds another engine (violates independence) — permanently.
- **Native-Rust-only product UI forever** — rejected: forfeits the
  TypeScript preference and the dogfooding benefit; retained as fallback.
- **WebStore-compatibility promise** — rejected: unclaimable honestly
  (dossier §2); a subset matrix is the honest form.

## Consequences

- The Phase 1 shell is modest by design; stakeholders must not read it
  as the product.
- Self-hosted chrome implies the engine needs the features chrome UIs
  use (scrolling, forms, layout) before the product UI lands — sequenced
  in the roadmap as a Phase 9–10 dependency, not a Phase 1 promise.
- The extension matrix is a maintenance obligation: it must track every
  implemented/partial/not-implemented decision.

## Revisit conditions

Phase 9 evidence on self-hosted chrome stability; extension-subset
adoption signals (if the chosen subset cannot host meaningful
extensions, expand the matrix with evidence); DevTools frontend license
review outcome.
