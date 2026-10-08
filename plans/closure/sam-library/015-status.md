# Milestone 015 status — Conformance DATAGRAM2 runner dispatch corrective

Status: **closed**.

Implementation revision: `75d1a13f7a2f09aea720c7489ec8d7ac4ab67317` (`fix: correct live SAM qualification findings`).
Closure revision: this record, committed immediately after the implementation revision.

Plan: `plans/implementation/sam-library/015-conformance-datagram2-runner-dispatch-corrective.md`.
Discovery context: M011's first full run against pinned i2pd.

## Work completed

- Added an explicit feature-name mapping for STREAM, DATAGRAM, RAW, DATAGRAM2, and
  DATAGRAM3 in both peer-present and peer-absent matrix construction.
- Replaced the DATAGRAM2/DATAGRAM3 unreachable branch with ordinary row construction.
- Added a runner test covering every session style.

## Verification

| Command or check | Result |
|---|---|
| `cargo test --locked -p i2pr-sam --bin sam-conformance` | pass — 1 runner test |
| `cargo test --locked --workspace --all-targets` | pass — 63 tests across 13 suites |
| Pinned i2pd full-plan qualification after correction | completed with schema-valid artifact; the run no longer panics at DATAGRAM2 |
| `scripts/check-conformance-artifact.py` | pass — artifact schema valid |

The full run also exposed i2pd's explicit unsupported PRIMARY verdict, handled by M016.
M011 remains blocked on concrete identities and payload exchanges.
