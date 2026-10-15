# Veyra Documentation Index

Reading order by purpose:

**Understand the project** →
1. [../README.md](../README.md) — identity, honest status
2. [research/README.md](research/README.md) — how research claims are evidenced
3. [architecture/system-overview.md](architecture/system-overview.md) — proposed design

**Contribute code** →
1. [../CONTRIBUTING.md](../CONTRIBUTING.md) — workflow + commands
2. [engineering/engineering-standards.md](engineering/engineering-standards.md) — the rules CI enforces
3. [architecture/dependency-graph.md](architecture/dependency-graph.md) — crate boundaries

**Make a consequential decision** →
[decisions/](decisions/) + [architecture/decision-log.md](architecture/decision-log.md)

**Plan / track work** →
[roadmap/implementation-plan.md](roadmap/implementation-plan.md) (phases + acceptance criteria),
[roadmap/compatibility-program.md](roadmap/compatibility-program.md) (test integration),
[roadmap/compatibility-matrix.md](roadmap/compatibility-matrix.md) (the truth surface)

**Security** →
[../SECURITY.md](../SECURITY.md) (reporting),
[architecture/process-and-security-model.md](architecture/process-and-security-model.md) (threat model + invariants)

## Document map

```
docs/
├── README.md                      (this file)
├── research/                      Chromium dossier + ecosystem audit (evidence-tiered)
│   ├── README.md                  index + notation
│   ├── chromium-architecture.md
│   ├── web-platform-and-rendering.md
│   ├── javascript-and-wasm.md
│   ├── security-model.md
│   ├── networking-and-storage.md
│   ├── browser-product-features.md
│   ├── testing-and-performance.md
│   ├── dependency-and-license-audit.md
│   ├── source-register.md         verified sources + access dates
│   └── coverage-matrix.md         investigated vs pending
├── architecture/
│   ├── system-overview.md
│   ├── process-and-security-model.md
│   ├── rendering-pipeline.md
│   ├── execution-and-web-apis.md
│   ├── dependency-graph.md
│   └── decision-log.md
├── decisions/                     11 ADRs + template
├── roadmap/
│   ├── implementation-plan.md     Phases 0–11 + acceptance criteria
│   ├── compatibility-program.md
│   └── compatibility-matrix.md
└── engineering/
    └── engineering-standards.md
```

## Maintenance rules

- Documentation changes are PRs like code changes; `veyra-doctor`
  validates structure on CI.
- Research/architecture/roadmap references stay consistent: if new
  evidence invalidates a decision, a superseding ADR is written (never
  silent edits) — see decision-log rules.
- Dates: every research claim carries its access date (source register);
  every matrix transition carries its evidence date.
