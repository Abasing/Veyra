# Architecture Decision Records

ADRs record consequential architectural choices with their evidence,
alternatives, and consequences. Read [`../architecture/decision-log.md`](../architecture/decision-log.md)
for the index and process.

## Template

```markdown
# ADR NNNN: <title>

- **Status:** Proposed | Accepted | Superseded by NNNN | Rejected
- **Date:** YYYY-MM-DD
- **Deciders:** <who>

## Context

<The forces at play. Cite research evidence with document + register IDs.>

## Decision

<What we do, stated so a new contributor could act on it.>

## Alternatives considered

<Each alternative with why it lost.>

## Consequences

<What this commits us to; what becomes harder; what gets easier.>

## Revisit conditions

<Metrics, dates, or events that reopen this decision.>
```

## Rules

- One ADR per decision; consequential = changes a subsystem boundary,
  adds a dependency, introduces native code, or alters the security model.
- Context must cite evidence (research dossier + source register IDs);
  decisions without evidence are Proposed, not Accepted.
- ADRs are immutable; supersession creates a new ADR.
