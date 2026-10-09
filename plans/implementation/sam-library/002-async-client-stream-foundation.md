# SAM Library Milestone 002 — Async client, Destination/NAMING, and STREAM foundation

Status: conditionally closed

Repository baseline: `956238fcced742836833befaf6e748e97f435001` (M001 implementation baseline).

Source roadmap:

- `plans/subsystems/sam-library-roadmap.md#002--async-client-destinationnaming-and-stream-foundation`

Long-term requirements:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`

Applicable ADRs:

- ADR-0001
- ADR-0002

Primary class: capability + infrastructure

## 1. Objective

Build the canonical async Rust client on the M001 protocol/state layer and close a real
ordinary STREAM path: connect to a bridge, negotiate HELLO, generate/use a Destination,
resolve names, create a STREAM session, connect outbound, accept inbound, exchange bytes,
and close/cancel deterministically.

This milestone establishes the public Rust shape that later datagram/shared-session work
extends.

## 2. Why this milestone is ready

Blocked until M001 closes.

At handoff, refresh:

- M001 public protocol types and limits;
- reference freeze revisions;
- any M001-discovered syntax/quirk corrections.

## 3. Current implementation evidence

Expected M001 substrate:

- socket-free typed SAM protocol;
- explicit control/data state transitions;
- `SamCapabilities`;
- clean-room reference freeze;
- workspace/lint/boundary guards.

M002 must not duplicate parsing or build commands from ad-hoc strings.

## 4. Invariants that must not regress

- one owner for each connection/session/task;
- no detached background task without cancellation and join ownership;
- bounded control-read buffer;
- timeouts are explicit and cancellation-aware;
- STREAM control socket becomes raw data only after a successful status;
- failed CONNECT/ACCEPT never exposes a partially initialized stream;
- private Destination/auth material is redacted;
- retry does not silently change Destination identity;
- public high-level API does not expose router-specific PRIMARY/MASTER quirks yet.

## 5. Scope

### In scope

- Tokio-based first production async transport unless M001 evidence justifies a simpler
  executor-neutral implementation;
- connection configuration and explicit deadlines;
- optional caller-supplied/preconnected async I/O seam if it can be done without
  destabilizing ownership, enabling future TLS/custom transports;
- HELLO negotiation;
- optional SAM authentication fields supported by negotiated syntax;
- DEST GENERATE;
- NAMING LOOKUP including typed not-found/error handling;
- ordinary STYLE=STREAM SESSION CREATE;
- `StreamSession`;
- outbound `connect()`;
- listener/accept abstraction over STREAM ACCEPT;
- authenticated remote Destination exposure on accepted streams when the protocol
  provides it;
- `AsyncRead`/`AsyncWrite` data phase;
- close/cancel/timeouts;
- deterministic mock bridge plus live interop.

### Explicitly out of scope

- PRIMARY/MASTER shared sessions;
- DATAGRAM/RAW/DATAGRAM2/3;
- STREAM FORWARD as a general host-forwarding API;
- silent reconnect;
- blocking/Python/C;
- tunnel daemon;
- service-tunnel policy integration;
- package publication.

## 6. Required production changes

### Async transport ownership

Add the async machinery to `i2pr-sam`, keeping `i2pr-sam-proto` runtime-neutral.

Prefer one connection abstraction that can wrap a Tokio TCP stream and, if practical,
caller-provided compatible I/O. Do not build a generalized runtime framework merely to
claim executor independence.

### Client and negotiation

Provide a semantic entry point such as `SamClient::connect(config)`.

Config must include:

- bridge endpoint;
- requested min/max SAM version;
- handshake/connect/control deadlines;
- optional auth credentials with redacted Debug;
- conservative buffer/resource ceilings where configurable.

HELLO result populates negotiated version but leaves optional capabilities Unknown until
evidenced.

### Destination and naming

Expose:

- destination generation with explicit signature type; default/recommended behavior must
  avoid legacy DSA by accident;
- private Destination import/use as an opaque secret-bearing type;
- public Destination type;
- name lookup with explicit exact-name input and result/error;
- optional lookup options only when syntax/capability evidence permits.

Do not implement I2P cryptography that the SAM bridge owns.

### STREAM session

Create one long-lived ordinary STREAM session around one Destination.

Session creation options must preserve arbitrary I2CP/streaming options in a bounded,
typed-safe representation without allowing reserved SAM keys to be duplicated.

### Connect

Each STREAM CONNECT uses a fresh bridge connection as required by SAM v3 semantics,
performs HELLO, issues CONNECT, consumes status, then hands the underlying stream to the
caller in data phase.

Destination, FROM_PORT/TO_PORT, silence options, and errors must be typed.

### Accept/listener

Expose a listener abstraction that can repeatedly create/drive ACCEPT connections without
forcing callers to manually perform SAM handshakes.

For non-silent ACCEPT, capture the authenticated remote Destination before entering byte
mode. Silent mode must not synthesize one.

### Lifecycle

Closing a session prevents new connects/accepts, cancels pending owned work, and lets
already-returned stream handles close according to documented semantics. Exact policy must
be tested and documented.

## 7. Ordered work packages

### WP A — async connection + HELLO

Acceptance: deterministic bridge tests for negotiation, auth redaction, timeout, malformed
reply, early EOF.

### WP B — Destination/NAMING

Acceptance: generate/lookup against mock plus live router; no secret logging.

### WP C — STREAM session create

Acceptance: long-lived control/session owner; duplicate ID and router errors typed.

### WP D — outbound connect/data transition

Acceptance: exact bytes exchanged after successful CONNECT; parser cannot consume payload.

### WP E — accept/listener

Acceptance: remote Destination semantics proven; cancellation of pending ACCEPT is clean.

### WP F — live interoperability

Run against at least current Java I2P plus one of current i2pd/i2pr. Prefer all three when
available; absence must be explicit, not silently skipped.

## 8. Failure, cancellation, restart, and contention semantics

- HELLO/session/connect deadlines are distinct.
- Cancellation during handshake closes that connection and returns no handle.
- Cancellation after data-phase handoff belongs to the caller's stream operation.
- Concurrent CONNECT/ACCEPT operations are bounded per session.
- Session close races with new operation admission deterministically: one wins through an
  atomic/generation state; no orphan connection remains.
- No automatic reconnect in M002.
- Loss of the session owner marks the session unusable; caller must explicitly recreate it.

## 9. Compatibility and migration

No pre-existing API compatibility.

Router result strings not recognized by the typed enum remain accessible as an Unknown
variant without panicking.

Live differences discovered here feed a centralized compatibility profile; do not scatter
router-name checks in connect/accept code.

## 10. Required tests

- handshake success/version mismatch/auth rejection;
- malformed/oversized replies;
- Destination generate and lookup;
- name not found vs bridge failure;
- ordinary session creation;
- outbound stream exact-byte round trip;
- inbound accept exact-byte round trip;
- non-silent peer Destination capture;
- silent accept cannot claim peer identity;
- session/control EOF;
- connect timeout;
- accept cancellation;
- concurrent bounded connect/accept;
- close/admission race;
- secret Debug/redaction;
- live Java + second-router matrix.

## 11. Required verification commands

At minimum M001 floor plus:

```bash
cargo test --locked -p i2pr-sam
cargo test --locked -p i2pr-sam --test stream_mock
# explicit environment-gated live interop commands established by the implementation
```

Missing live-router prerequisites must make an explicitly requested interop command fail
with a clear reason; ordinary unit CI may skip separately marked live tests.

## 12. Documentation updates

- async API examples;
- bridge configuration/security note;
- lifecycle/cancellation semantics;
- compatibility matrix rows actually exercised;
- roadmap/registry/closure.

## 13. Acceptance criteria

1. Real HELLO negotiation works.
2. Destination generation/import and naming are typed/redacted.
3. Ordinary STREAM session is long-lived and bounded.
4. CONNECT produces a real byte stream only after OK status.
5. ACCEPT produces a real byte stream and truthful peer identity semantics.
6. Timeouts/cancellation/close have tested ownership.
7. Parser/state logic remains in protocol crate.
8. Java plus at least one second router pass the ordinary STREAM matrix.
9. No datagram/shared-session capability is claimed.

## 14. Stop conditions

Stop if:

- M001 state model cannot express real STREAM transitions without an architectural rewrite;
- live routers disagree on ordinary SAM 3.1 STREAM behavior in a way requiring public API
  divergence rather than a compatibility adapter;
- cancellation cannot be made leak-free under the chosen runtime ownership;
- supporting TLS/custom transport requires protocol-layer runtime coupling.

## 15. Closure evidence required

Exact router/release revisions, negotiated versions, live rows, timeout/cancel tests,
resource ceilings, public API inventory, dependency tree, full verification commands, and
any router quirk profile introduced.

## 16. Handoff notes

M003 will add datagrams and shared sessions. Avoid naming ordinary-session types in a way
that makes shared sessions impossible to add without breaking the API.

Closure evidence: `plans/closure/sam-library/002-status.md`. Java I2P plus a second-router
live STREAM run remains an explicit residual condition.
