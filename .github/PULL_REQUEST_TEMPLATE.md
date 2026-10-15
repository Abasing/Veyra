## Summary

<!-- What does this PR change and why? One paragraph. -->

## Evidence

- [ ] Checks run locally and pass: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo run -p veyra-doctor`, `python3 -m pytest tests/ -q`, `python3 -m ruff check tools/ tests/`
- [ ] Compatibility-matrix rows updated (with evidence) if behavior claims changed
- [ ] Performance-sensitive change: baselines re-measured and recorded
- [ ] Claims are backed by test/suite evidence, not demonstration pages

## Security

- [ ] Touches untrusted-input parsing → fuzz target updated and run
- [ ] Crosses a trust boundary / adds an IPC message → security-model invariant cited
- [ ] Adds a dependency → license in `deny.toml` allow-list, ADR-0001/0007 satisfied, audit note added
- [ ] No secrets, credentials, or machine-specific files included

## Docs & decisions

- [ ] Docs updated (research claims cite the source register with access dates)
- [ ] ADR added/updated if this changes a subsystem boundary, dependency, or the security model
- [ ] `veyra-doctor` passes (structure checks are enforced in CI)
