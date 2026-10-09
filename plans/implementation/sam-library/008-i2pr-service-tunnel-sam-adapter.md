# SAM Library Milestone 008 — i2pr service-tunnel SAM adapter

Status: closed

Repository baseline: `8982cb7` (M007 closed).

Source roadmap: `plans/subsystems/sam-library-roadmap.md#008--i2pr-service-tunnel-adapter`.

External contract: i2pr Plan 379 merge `e13546b` and
`i2pr-service-tunnels` revision
`f0fb74a8582d6077a7c5db49d613699688115bad`.

## 1. Objective

Add a separate SAM transport adapter crate that consumes the public
`i2pr-service-tunnels` policy/filter core. Preserve explicit Destination group identity,
use the canonical SAM client for STREAM transport, and surface authenticated inbound peer
identity only from a SAM accepted stream.

## 2. Why this milestone is ready

M011 and M013 are closed, M017 is merged, and M007 is closed. Upstream Plan 379 merged the
public consumer contract to i2pr `main`; the exact core revision selected here is
`f0fb74a8582d6077a7c5db49d613699688115bad`, which is the revision proven by upstream's
current external-consumer fixture.

## 3. Invariants

- Do not copy policy/parser code from i2pr; call its public API through a pinned Git
  dependency.
- Equal explicit destination group keys map to one SAM shared-session owner; dedicated
  keys remain distinct.
- No grouping is inferred from service kind, listener, destination, or target.
- Inbound authorization/rate policy receives only the authenticated Destination parsed
  from a non-silent SAM accept and its canonical 32-byte hash.
- Persistent/key-reference policies fail closed until an identity store is supplied;
  generated ephemeral identity is never presented as persistent.
- SAM runtime/session ownership remains in `i2pr-sam`; this crate owns adapter-level
  policy composition and bounded stream forwarding.

## 4. Scope

In scope: `i2pr-sam-service-tunnels`; pinned public-core dependency; validation and
collision-free group planning; GenericClient and GenericServer STREAM session binding;
outbound peer resolution; authenticated inbound peer/access-policy mapping; bounded
HTTP-server request-head filtering through the core; and deterministic mock-bridge,
grouping, identity, and filtering tests.

Out of scope: daemon configuration/persistence, key storage, OS listener lifecycle,
Streamr/datagram transport, complete SOCKS/IRC/CONNECT profiles, full HTTP body filtering,
crate publication, and multi-router live qualification. Unsupported profiles and
persistence requirements must be refused explicitly.

## 5. Ordered work packages

1. Pin and compile the upstream crate at the exact revision above; document the public
   contract and dependency provenance.
2. Validate sets, reject unsupported/persistent specs, and derive destination-group plans
   using the core's `group_key` API.
3. Build dedicated or shared SAM STREAM sessions for enabled supported specs; connect to
   validated/resolved peers and accept server streams.
4. Convert authenticated peer Destinations to core hash inputs, enforce server access
   policy, and delegate HTTP server request filtering to the pinned core.
5. Add bounded stream-to-loopback-TCP forwarding and deterministic end-to-end mock tests.

## 6. Failure and lifecycle semantics

Validate the full set before opening SAM sessions. Session/group creation is ordered and
failure aborts construction, dropping already-created handles. Shared owner loss closes
all child streams through the existing SAM lifecycle. A missing authenticated peer identity
or denied access closes the stream before local forwarding. Per-connection forwarding is
bounded by the validated service connection limit, byte-buffer ceiling, and configured
connect/read/write/shutdown deadlines.

## 7. Required verification

Run the workspace stable/MSRV floor, clippy, docs, API and protocol-boundary guards,
upstream dependency/build checks, adapter unit/mock tests, and a CARGO_TARGET_DIR-isolated
external consumer build using only the public `i2pr-sam-service-tunnels` package. No live
router claim is required by this adapter milestone.

## 8. Acceptance criteria

- The adapter builds with the exact pinned public core revision and contains no copied core
  implementation.
- Equal explicit groups share exactly one SAM owner and distinct/dedicated groups never
  collapse.
- The supported client/server STREAM slice works through a mock bridge; server access
  policy receives only authenticated peer hashes.
- HTTP request filtering calls the pinned core and preserves its bounded refusal results.
- Unsupported profiles, missing peers, denied peers, and persistent identities without a
  store fail deterministically before forwarding.
- Verification and closure record exact revision/commands/results and successor readiness.

## 9. Stop conditions

Stop and revise this plan if the pinned public API cannot express a required seam without
copying policy, if the SAM API cannot supply authenticated inbound identity, or if shared
groups cannot preserve the core's explicit linkability contract.

## 10. Closure evidence

See `plans/closure/sam-library/008-status.md` for implementation SHA, exact upstream
revision, adapter-supported profile subset, group/identity evidence, filtering calls,
commands actually run, residual unsupported profiles, and M009 readiness.
