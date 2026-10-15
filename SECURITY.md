# Security Policy

## Project status

Veyra is an **early-stage, independently implemented browser-engine project**.
There is currently no released browser software and no engine implementation,
so there is **no attack surface in released artifacts today**. This policy
governs the repository, its tooling, and all pre-release development.

## Reporting a vulnerability

Report security issues through GitHub's private vulnerability reporting:
open the repository's **Security** tab and use **Report a vulnerability**.
This project does not yet operate a dedicated security email address; do not
file public issues for security-sensitive topics.

Please include: affected commit hash or workflow file, a minimal reproduction
or proof of concept, expected vs. actual behavior, and your assessment of
impact. For build/CI issues (e.g., workflow privilege escalation, secret
exposure), include the exact job and step.

## What is in scope

- Repository automation: GitHub Actions workflows, action pinning, workflow
  permissions, template injection in steps, unsafe pull-request execution.
- Build and validation tooling in `tools/` and `tests/`, including dependency
  supply-chain issues surfaced by `cargo-deny` or dependency review.
- Documentation that could cause unsafe integration decisions if wrong
  (e.g., incorrect statements in the security model or threat model).
- Any future engine code, with the threat model in
  [`docs/architecture/process-and-security-model.md`](docs/architecture/process-and-security-model.md)
  as the reference for what counts as a boundary violation.

## What is out of scope

- Claims that Veyra "is not a real browser" — that is the documented status.
- Social engineering against project maintainers or third-party services.
- Findings in GitHub's own infrastructure (report to GitHub).

## Handling expectations

- Triage within 7 days; substantive response within 14 days. These are project
  commitments while the codebase is small; they will be re-published with
  specific service levels once releases exist.
- Accepted issues are fixed on `develop` and credited in the fix commit unless
  the reporter prefers anonymity.
- Coordinated disclosure: details are published after a fix lands, or at
  90 days maximum, whichever comes first, unless the reporter asks for more
  time in writing.

## Security engineering baseline

Security-relevant engineering rules (panic policy, unsafe Rust, untrusted-input
fuzzing requirements, capability-based internal APIs, dependency rules) live in
[`docs/engineering/engineering-standards.md`](docs/engineering/engineering-standards.md)
and are enforced as far as the current codebase allows. The architecture-level
threat model lives in
[`docs/architecture/process-and-security-model.md`](docs/architecture/process-and-security-model.md).

## Safe harbor

We consider good-faith security research of this repository helpful and will
not pursue action against it, provided it does not degrade service for other
users, does not exfiltrate data, and stops at the boundary of proof.
