# Milestone 013 status — Blocking parity and foundation closure-truth corrective

Status: **closed**.

Implementation revision: `97d5a9d0b9e59e6e5d58a5add1b169c4f20327ea` (`docs: remove unavailable live Actions workflow`).
Closure revision: this record, committed immediately after the implementation revision above.
Hosted CI evidence: workflow run [37718765511](https://github.com/dbowm91/i2pr-sam/actions/runs/37718765511), conclusion **success**, dispatched against implementation revision `97d5a9d0b9e59e6e5d58a5add1b169c4f20327ea`. All six jobs passed: fmt, clippy, test-linux, MSRV 1.89, macOS, and Windows. This record is documentation-only and adds no implementation changes.

Plan: `plans/implementation/sam-library/013-blocking-parity-and-foundation-closure-truth-corrective.md`.
Historical authority corrected additively: `plans/closure/sam-library/006-status.md`.

## 1. Current-authority correction

M006's implementation is preserved, as is its historical claim that blocking parity was
met. At the time, the checked-in blocking suite did not support that breadth. M013 supplies
the missing evidence with ten new blocking parity tests reusing the async crate's scripted
bridge fixture. M006's closure record is unchanged.

No blocking-only SAM parser, protocol state machine, or session implementation was added.
The blocking facade continues to call the canonical async client.

## 2. Blocking test inventory

`cargo test --locked -p i2pr-sam-blocking --all-targets`: **12 tests passed** at this
revision: the two existing STREAM framing tests and ten new tests in
`crates/i2pr-sam-blocking/tests/parity.rs`.

| Coverage | Evidence |
|---|---|
| Destination generation, lookup, typed peer resolution, and capability state | Typed public values, redacted generated-secret Debug, Destination and base32 hash variants; generation and lookup failures remain errors |
| Ordinary DATAGRAM1 and RAW control-socket transport | Exact outgoing headers and raw payload bytes; authenticated DATAGRAM1 source and RAW protocol/port metadata preserved |
| UDP-forwarded DATAGRAM1, RAW, DATAGRAM2, and DATAGRAM3 | Payload exchange over loopback with source trust asserted; DATAGRAM3 remains an unverified 32-byte hash; RAW metadata is retained with `HEADER=true` |
| DATAGRAM2/3 transport refusal | Legacy control-socket selection returns `Unsupported` before opening a session control socket |
| Shared owner and child identity | Concrete owner identity and equal child identities |
| Shared STREAM connect and accept | Connect payload plus non-silent accept peer Destination and payload framing |
| Shared datagram child and lifecycle | UDP payload framing; duplicate child ID rejection; remove child; removed child becomes unusable; sibling remains usable; owner close invalidates children |
| Runtime and errors | Nested Tokio invocation returns `NestedRuntime`; `INVALID_STYLE` maps to `Unsupported` and records capability unsupported; transient peer rejection remains `Rejected` |
| Receive timeout and close | Stream read and datagram receive timeouts; close is idempotent; timed-out read does not produce a later successful read |

The public API snapshot remains unchanged at 347 declarations.

## 3. Verification

Commands run on implementation revision `97d5a9d0b9e59e6e5d58a5add1b169c4f20327ea`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo test --locked -p i2pr-sam-blocking --all-targets` | pass — 12 blocking tests |
| `cargo test --locked --workspace --all-targets` | pass — 61 tests across 13 suites |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` | pass |
| `python3 scripts/check-api-snapshot.py` | pass — no public API drift |
| `python3 scripts/check-api-snapshot.py --self-test` | pass — 9/9 |
| `python3 scripts/check-proto-boundary.py` | pass |
| `python3 scripts/check-proto-boundary.py --self-test` | pass — 29/29 |
| `python3 scripts/check-conformance-artifact.py --self-test` | pass — all cases |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | pass — 11 harness tests |

Hosted CI run `37718765511` completed successfully against `97d5a9d0b9e59e6e5d58a5add1b169c4f20327ea`. It includes Linux
stable, Rust 1.89 MSRV, macOS, Windows, Clippy, and formatting.

## 4. Closure and successor audit

M013 closes the deterministic blocking-parity evidence gap and corrects M006's current
authority additively. It does not supply live router evidence. M012 and M013 are now closed;
M011 is the sole remaining foundation gate, blocked on suitable two-router provisioning
and successful live payload rows. M007 bindings and M008 service-tunnel integration remain
blocked until strict foundation closure; repository license selection also remains a
publication prerequisite for M007.
