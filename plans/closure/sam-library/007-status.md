# Milestone 007 status — C ABI and Python bindings

Status: **closed**.

Repository baseline: `84c5727` (`main` before M007).

Implementation revision: `01c0d7c158b6fbf5312a857fed38c3f3d29a274b` (M007 bindings,
documentation, tests, and active-plan registration).

Plan: `plans/implementation/sam-library/007-c-abi-and-python-bindings.md`.

## 1. Work completed

- Added `i2pr-sam-ffi` as `cdylib` and `staticlib`, with a checked-in C header. The
  initial API connects to a SAM bridge, resolves a name, explicitly generates a
  Destination, and closes a client.
- C uses monotonic integer handles and caller-owned byte buffers. Inputs are capped at
  64 KiB and UTF-8 checked; return codes are stable and panics are contained at every
  export. A registry lookup clones the handle's `Arc` before blocking work, so close
  cannot hold the registry lock during network operations.
- Added `i2pr-sam-python` as an abi3 Python extension over the same blocking facade. It
  exposes `Client`, `lookup`, `generate_destination`, `ValueError` for malformed endpoints,
  and `SamError` for operational errors.
- Added local maturin build metadata and binding ownership/security documentation. No
  package was published and no license was selected.
- No SAM protocol, router compatibility, or Rust client semantics changed. Existing Rust
  public API snapshot remains at 347 declarations with zero drift.

## 2. Verification

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo check --locked --workspace --all-targets` | pass |
| `cargo test --locked --workspace --all-targets` | pass — 71 tests across 16 suites |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo doc --locked --workspace --no-deps` | pass |
| `cargo +1.89 check --locked --workspace --all-targets` | pass |
| `cargo +1.89 test --locked --workspace --all-targets` | pass — 71 tests |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | pass — 15 tests |
| API snapshot comparison | pass — 347 declarations, zero drift |
| API snapshot self-test | pass — 9/9 |
| protocol-boundary guard | pass |
| protocol-boundary self-test | pass — 29/29 |
| C header syntax check | pass — `cc -Icrates/i2pr-sam-ffi/include -fsyntax-only` |
| C ABI mock bridge test | pass — HELLO, lookup, Destination generation, close and stale-handle refusal |
| Python module mock bridge test | pass — module initialization, lookup, Destination generation, malformed endpoint and operational exceptions |
| maturin wheel build and isolated import | pass — CPython 3.12, abi3 Python 3.9+ |
| Java I2P M011 artifact validator | pass — existing artifact revalidated; no live campaign run |

The Python wheel was installed under `/tmp` for import verification; no generated Python
artifacts were added to the repository.

## 3. ABI contract and residual risks

- C callers must provide valid readable/writable memory matching each pointer and length;
  the ABI rejects null required pointers, invalid UTF-8, over-limit input, unknown handles,
  and undersized outputs.
- C return code 255 identifies a contained Rust panic. The ABI does not expose panic text.
- The binding surface intentionally covers connection, lookup, and Destination generation;
  session and datagram APIs remain available only through Rust pending a separate reviewed
  binding expansion.
- The generated private Destination is returned only by the explicit generation operation.
  Applications are responsible for protecting it after receipt.
- Local wheel/ABI packaging is verified, but publication and platform-specific binary
  distribution remain gated on later release and license decisions.

## 4. Successor unblock audit

- **M008 service-tunnel adapter** — ready to plan with upstream
  `i2pr-service-tunnels` revision
  `f0fb74a8582d6077a7c5db49d613699688115bad` (Plan 379 merge `e13546b` contains this
  public core revision and the current external-consumer fixture). → **ready**.
- **M009 daemon/config/management API** — depends on M008. → **blocked**.
- **M010 CLI/WebUI/sidecar packaging** — depends on M009. → **blocked**.
