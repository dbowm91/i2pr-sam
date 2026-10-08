# Milestone 014 status — SAM bridge preflight greeting corrective

Status: **closed**.

Implementation revision: `75d1a13f7a2f09aea720c7489ec8d7ac4ab67317` (`fix: correct live SAM qualification findings`).
Closure revision: this record, committed immediately after the implementation revision.

Plan: `plans/implementation/sam-library/014-sam-bridge-preflight-greeting-corrective.md`.
Discovery context: M011 live-router provisioning.

## Work completed

- `probe_bridge()` now sends the standard `HELLO VERSION MIN=3.0 MAX=3.3` request.
- Its regression test asserts the exact request bytes.
- Both live bridges answered `HELLO REPLY` to the corrected preflight: Java I2P at
  `127.0.0.1:7656` and pinned i2pd at `127.0.0.1:19856`.

## Verification

| Command or check | Result |
|---|---|
| `python3 -m unittest discover -s tests -p 'test_*.py'` | pass — 11 tests |
| `qualify.probe_bridge(127.0.0.1:7656)` | pass — `HELLO REPLY` |
| `qualify.probe_bridge(127.0.0.1:19856)` | pass — `HELLO REPLY` |
| M011 qualification preflight, Java I2P and i2pd | pass — both lanes reached the conformance runner |
| `git diff --check` | pass |

M014 is closed. M011 remains blocked on usable cross-router tunnels and peer identities;
this correction only enables the live harness to recognize SAM bridges correctly.
