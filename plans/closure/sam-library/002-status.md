# SAM Library M002 closure

Status: **conditionally closed**.

Implementation SHA: `956238fcced742836833befaf6e748e97f435001`.

## Evidence

- The client implements bounded HELLO, credentials redaction, DEST GENERATE, NAMING
  LOOKUP, ordinary STREAM session creation, outbound CONNECT, inbound ACCEPT, status to
  byte-phase handoff, peer Destination capture, explicit close, and a 64-operation
  semaphore bound.
- The control reader retains bytes buffered beyond `STREAM STATUS`; mock tests send status
  and application bytes in one write and verify no data is lost.
- Mock suites exercise STREAM CONNECT and ACCEPT payload exchange, lookup, generated
  Destination redaction, malformed and oversized replies, timeouts, and 100 session
  create/close cycles with control-socket EOF checks.
- Capability state remains independent from HELLO version and is updated only after
  successful style/session operations.

## Commands executed

The stable and Rust 1.89 workspace check, test, and Clippy commands listed in M001 closure
all passed at implementation SHA above. `rtk cargo fmt --all --check` and
`rtk env RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` also passed.
The final stable workspace run reported 18 tests passed.

## Router matrix

| Router pin | STREAM handshake/session/payload | Result |
|---|---|---|
| Java I2P `a629ec7c9c675dd252d005fb881efd92e8e6ba27` | not exercised | not_run |
| i2pd `d147bb0fd6789c75dc1c4d70c4f79b151a552d53` | not exercised | not_run |
| i2pr `4f4e98f5a0241355af5099c73065088dede1b1de` | not exercised | not_run |

The exact attempts and environment evidence are in `specs/live-router-qualification.md`.
All three connections to `127.0.0.1:7656` were refused. There is no locally installed
i2pd, no router process, and Docker daemon access is denied. These are unavailable
prerequisites, not compatibility failures.

## Deviations and residual risk

Live Java I2P plus a second router is required before an unconditional close. The current
mock does not exercise authentication rejection, concurrent accept contention, configured
silent accepts, a real router Destination, or cancellation against a router. There is no
TLS/custom-I/O seam. `ReconnectPolicy` added in M004 applies only to bridge connection
establishment; session identity is never silently recreated.

## Successor audit

M003 was conditionally unblocked because the async ownership API and deterministic bridge
contracts are implemented. Live router qualification remained open and is not represented
as a pass. This allowed work on the independently testable datagram and shared-owner
surfaces to proceed.
