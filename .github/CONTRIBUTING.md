# Contributing

## CI lanes and their local equivalents

Every lane in `.github/workflows/ci.yml` runs a command you can run locally with no
arguments. If a lane fails on a pull request, reproduce it locally before changing code.

| Hosted lane | Local command |
| --- | --- |
| `fmt` | `cargo fmt --all -- --check` |
| `clippy` | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| `test-linux` (tests) | `cargo test --locked --workspace` |
| `test-linux` (guards) | `python3 scripts/check-proto-boundary.py` |
| `test-linux` (boundary self-test) | `python3 scripts/check-proto-boundary.py --self-test` |
| `test-linux` (API self-test) | `python3 scripts/check-api-snapshot.py --self-test` |
| `test-linux` (artifact validator self-test) | `python3 scripts/check-conformance-artifact.py --self-test` |
| `msrv` | `cargo +1.89.0 build --locked --workspace --all-targets` then `cargo +1.89.0 test --locked --workspace` |
| `test-macos` | `cargo build --locked --workspace --all-targets` then `cargo test --locked --workspace` on macOS |
| `test-windows` | `cargo build --locked --workspace --all-targets` then `cargo test --locked --workspace` on Windows |

Notes:

- The guard and validator scripts are pure standard library. No `pip install` is needed.
- `cargo test --locked --workspace` uses deterministic mock-bridge tests only. It needs
  no I2P router and no network beyond crates.io.
- `--locked` is mandatory: CI must not silently resolve different dependency versions
  than the committed `Cargo.lock`.
- `msrv` pins Rust `1.89.0` (the workspace `rust-version`) and asserts the resolved
  compiler version before building, so the lane cannot pass on a silently newer toolchain.
- `api/public-types.txt` comparison (`python3 scripts/check-api-snapshot.py`) is
  currently an **informational, non-blocking** step while the snapshot is regenerated.
  The `--self-test` variant above is required. When you change the public API, regenerate
  the snapshot with `python3 scripts/check-api-snapshot.py --write` and commit it in the
  same change, then the comparison can be promoted back to a required lane.

## Why live interoperability is a separate manual workflow

`.github/workflows/live-interop.yml` is `workflow_dispatch` only. It is deliberately
not part of ordinary CI, for three reasons:

1. **Ordinary CI must not depend on external I2P availability.** A push-triggered lane
   that needs a running router would make every commit's correctness contingent on
   somebody's network and router uptime.
2. **Live evidence must be attributable.** `plans/003-planning-process.md` requires that
   interoperability claims name the router and revision they came from. A hand-triggered
   run with `router` and `endpoint` inputs produces exactly that record.
3. **A blocked environment is not a result.** Where no router can run, `qualify.py`
   exits `3`, and the workflow reports a neutral `blocked` conclusion. It never reports a
   pass, and it never fails the job for a missing router.

Run it locally first:

```bash
python3 scripts/interop/udp_egress_probe.py
python3 scripts/interop/qualify.py --router i2pr --endpoint 127.0.0.1:7656 --plan stream
python3 scripts/check-conformance-artifact.py artifacts/interop/i2pr-conformance.json
```

`qualify.py` exit codes: `0` lanes ran with no `fail` rows, `1` a real `fail` row or an
invalid artifact, `3` blocked by the environment. Only `1` is a compatibility failure.

Note that `qualify.py` does not currently accept a `--peer-endpoint` flag; it builds the
runner command line itself. Peer-observed payload lanes therefore report as skipped.