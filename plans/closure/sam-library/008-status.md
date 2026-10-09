# Milestone 008 status — i2pr service-tunnel SAM adapter

Status: **closed**.

Repository baseline: M017 and M007 closed; adapter implementation branch
`plans/007-008-downstream-capabilities`.

Implementation revision: `041c82b05691a7c8fe662c6caa3d6aadab9c9047`.

Plan: `plans/implementation/sam-library/008-i2pr-service-tunnel-sam-adapter.md`.

Upstream contract: i2pr Plan 379 merge `e13546b`; exact pinned
`i2pr-service-tunnels` revision
`f0fb74a8582d6077a7c5db49d613699688115bad`.

## 1. Work completed

- Added `i2pr-sam-service-tunnels`, which uses the upstream public service-tunnel core for
  full-set validation, enabled-profile group planning, HTTP request parsing/filtering,
  server access policy, and peer rate limiting. Policy/parser code was not copied.
- Added the supported first slice: `GenericClient`, `GenericServer`, and `HttpServer` over
  SAM STREAM; unsupported profiles, non-default Destination crypto policy, and unsupported
  server targets are rejected explicitly.
- Equal explicit destination groups get one retained SAM shared-session owner. Server
  children retain their inbound options; wildcard server children may serve clients in the
  same group, while client-only groups receive one outbound child. Dedicated groups use
  separate SAM stream sessions.
- Persistent groups use the adapter-owned `DestinationIdentityStore` seam. Missing stores
  fail before SAM connection; generated keys are stored before sessions are created. This
  crate does not choose a storage path.
- Client STREAM resolves a core `DestinationRef` through SAM. Server STREAM accepts with
  non-silent peer identity, hashes only the authenticated SAM Destination, then applies
  core allow-list and rate policy before forwarding.
- HTTP request heads are bounded and filtered by the pinned core before forwarding to
  loopback TCP. Per-service concurrency, buffer ceilings, and configured connect/read/
  write/shutdown deadlines bound the forwarding path.
- Added deterministic mock-bridge tests and an isolated consumer fixture that imports only
  the adapter package. Added public scope and limitations documentation.

## 2. Verification

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo check --locked --workspace --all-targets` | pass |
| `cargo test --locked --workspace --all-targets` | pass — 76 tests across 17 suites |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo doc --locked --workspace --no-deps` | pass |
| `cargo +1.89.0 check --locked --workspace --all-targets` | pass |
| `cargo +1.89.0 test --locked --workspace --all-targets` | pass |
| `python3 scripts/check-api-snapshot.py` | pass — 347 declarations, zero drift |
| `python3 scripts/check-api-snapshot.py --self-test` | pass — 9/9 |
| `python3 scripts/check-proto-boundary.py` | pass |
| `python3 scripts/check-proto-boundary.py --self-test` | pass — 29/29 |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | pass — 15 tests |
| isolated external consumer check with `CARGO_TARGET_DIR=/tmp/i2pr-sam-m008-external-target` and fixture manifest | pass |

Adapter mock tests prove explicit/dedicated group mapping, client connection over a shared
owner, authenticated peer hash mapping, early profile/identity refusal, and delegation of
HTTP request filtering. No live-router interoperability run was performed or claimed.

## 3. Supported subset and remaining limits

Only GenericClient, GenericServer, and HttpServer profiles are adapted. Streamr/datagram,
SOCKS, IRC, CONNECT, full HTTP body policy, daemon configuration/management, durable key
storage, and OS listener lifecycle remain outside this milestone. Applications supply
their identity store and listener lifecycle. The SAM shared session implementation remains
the source of owner/child lifecycle behavior.

## 4. Successor readiness

- **M009 daemon/config/persistence/management API** — its M008 dependency is now closed;
  it is ready to plan. This milestone does not create an M009 implementation handoff.
- **M010 CLI/WebUI/sidecar packaging** — remains dependent on M009 and is not ready.
