# Milestone 016 status — i2pd unknown-style verdict corrective

Status: **closed**.

Implementation revision: `75d1a13f7a2f09aea720c7489ec8d7ac4ab67317` (`fix: correct live SAM qualification findings`).
Closure revision: this record, committed immediately after the implementation revision.

Plan: `plans/implementation/sam-library/016-i2pd-unknown-style-verdict-corrective.md`.
Discovery context: M011's full pinned-i2pd matrix.

## Work completed

- The client now recognizes only `I2P_ERROR MESSAGE="Unknown STYLE"` as an unsupported
  style verdict, alongside the standard `INVALID_STYLE` and `INVALID_ID` results.
- This verdict updates the affected capability and becomes `SamError::Unsupported`.
- Other `I2P_ERROR` replies remain ordinary rejections.
- Added a SAM-client regression test for i2pd's response form.

## Verification

| Command or check | Result |
|---|---|
| `cargo test --locked --workspace --all-targets` | pass — 63 tests across 13 suites |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass |
| Python harness suite | pass — 11 tests |
| API snapshot | pass — 347 rows unchanged |
| protocol-boundary self-test | pass — 29/29 |
| conformance-artifact self-test | pass — all cases |
| pinned i2pd full-plan artifact | schema-valid; PRIMARY is `unsupported`, `fail=0`, `not_run=8` |

M016 is closed. The eight `not_run` rows still require M011's live peer identity and tunnel
evidence; M016 does not promote M011 or the foundation to strict closure.
