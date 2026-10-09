# SAM Library Milestone 003 — Datagram and shared-Destination interoperability

Status: conditionally closed

Repository baseline: `956238fcced742836833befaf6e748e97f435001` (M002 implementation baseline).

Source roadmap:

- `plans/subsystems/sam-library-roadmap.md#003--datagram-and-shared-destination-interoperability`

Primary class: capability + invariant

## 1. Objective

Complete the modern SAM data-plane surface required by sophisticated applications:
legacy authenticated DATAGRAM, RAW, DATAGRAM2, DATAGRAM3, and one shared Destination
hosting multiple STREAM/datagram children through the deployed PRIMARY/MASTER variants.

The milestone must make authentication and compatibility differences explicit and prove
them against current Java I2P, i2pd, and i2pr rather than inferring support from HELLO.

## 2. Why this milestone is ready

Blocked until M002 closes with a stable ordinary-session client and live STREAM evidence.

At implementation start, refresh the official SAM spec and the exact current router
revisions. In particular, re-check i2pd issue #2303 / corresponding code: at registration
its final maintainer disposition was that PRIMARY would not be added while the other
shared-session work should function.

## 3. Current implementation evidence

Expected M002 substrate:

- negotiated async client;
- Destination/naming;
- ordinary STREAM session;
- connection/lifecycle ownership;
- capability state;
- deterministic mock bridge;
- live router harness.

Reference projects may show different API choices. They are behavior/API oracles only.

## 4. Invariants that must not regress

- legacy DATAGRAM and DATAGRAM2 authenticated source types are not interchangeable with
  DATAGRAM3's unverified source hash.
- RAW exposes no source identity.
- one `SharedSession` semantic owner maps to exactly one Destination/linkability domain.
- child IDs are distinct and lifetime-bound to the shared owner.
- a child cannot silently create a separate Destination.
- PRIMARY vs MASTER stays an internal compatibility dialect.
- version=3.3 does not imply any optional style is supported.
- host UDP forwarding is not the only representation of datagram semantics.
- all payload/queue/session counts are bounded.
- shared-owner loss invalidates children deterministically.

## 5. Scope

### In scope

- ordinary DATAGRAM, RAW, DATAGRAM2, DATAGRAM3 session types where supported;
- SAM datagram send/receive modes required by current spec/router behavior;
- local SAM UDP forwarding where applicable;
- control-socket receive/send forms where the selected style/spec supports them;
- I2P FROM_PORT/TO_PORT/PROTOCOL metadata;
- typed authenticated/unverified/no-source receive values;
- `SharedSession` and child add/remove;
- PRIMARY and MASTER dialect compatibility;
- same-Destination proof across STREAM + datagram children;
- feature observation/profile update;
- live Java/i2pd/i2pr conformance matrix;
- negative cross-session/lifecycle tests.

### Explicitly out of scope

- application-level authentication for DATAGRAM3;
- I2P BitTorrent DHT logic;
- I2CP implementation;
- host-facing tunnel daemon;
- service-tunnel adapter;
- Python/C/blocking APIs;
- auto-enabling unsupported router features;
- package publication.

## 6. Required production changes

### Datagram semantic types

Expose distinct receive shapes, for example conceptually:

```text
AuthenticatedDatagram {
  source: Destination,
  from_port,
  to_port,
  payload
}

UnverifiedDatagram3 {
  source_hash: UnverifiedSourceHash,
  from_port,
  to_port,
  payload
}

RawDatagram {
  from_port,
  to_port,
  protocol,
  payload
}
```

The concrete API may differ but must preserve those trust distinctions in the type system
or equally strong semantics.

### Datagram transport modes

Model local SAM bridge transport separately from I2P datagram semantics.

Support the spec/router-required combination of:

- SAM UDP bridge ingress/forwarding;
- control-connection receives when no forwarding port is configured;
- legacy DATAGRAM SEND/RAW SEND where defined.

DATAGRAM2/3 must not be forced through legacy control commands that do not support them.

Bind UDP sockets conservatively; default local forwarding listeners must be loopback.
Remote-bridge UDP forwarding needs explicit configuration and documentation because local
MTU/security behavior differs.

### Shared session

Expose one semantic `SharedSession` owning:

- one Destination;
- one control connection;
- bounded child registry;
- child styles/options;
- lifecycle generation.

Children may include STREAM, DATAGRAM, RAW, DATAGRAM2, DATAGRAM3 when the router proves
support.

### PRIMARY/MASTER compatibility

Represent a private/internal dialect enum. The public user asks for a shared session, not
a spelling.

At implementation start execute a small conformance probe against pinned Java/i2pd/i2pr
versions. Choose the Auto policy based on evidence. If current evidence remains that
MASTER is accepted broadly while i2pd rejects PRIMARY, prefer the broad interoperable
dialect or a deterministic fallback that cannot leave duplicate sessions.

A failed first attempt must be proven side-effect-free before an automatic second dialect
is attempted. Otherwise require explicit router profile rather than risky fallback.

### Capability learning

Optional support transitions from Unknown to Supported only after:

- successful operation/probe with no leaked resource; or
- a pinned router profile whose use is explicit and test-backed.

Unsupported results update capability state only when the router response unambiguously
means unsupported, not for transient I2P errors.

## 7. Ordered work packages

### WP A — ordinary datagram API and bounds

Implement legacy DATAGRAM/RAW, then D2/D3, with typed source trust.

### WP B — datagram transport modes

Implement SAM UDP/control paths and port/protocol metadata without conflating local ports
with I2P ports.

### WP C — shared owner + child state

Implement add/remove and lifetime semantics independent of PRIMARY/MASTER spelling.

### WP D — compatibility dialect

Implement evidence-based MASTER/PRIMARY selection and quirk profile.

### WP E — shared-Destination matrix

Create two shared sessions and prove STREAM plus datagram children exchange data while
reporting the same local Destination per owner.

### WP F — live router qualification

Run Java I2P, i2pd, and i2pr. Record every unsupported row instead of weakening the test
to the common denominator.

## 8. Failure, cancellation, restart, and contention semantics

- child add is transactional: failure leaves no live child handle/registry entry;
- duplicate listen port/protocol conflicts are rejected before or by bridge and reflected
  consistently;
- child remove is idempotent at the local semantic layer;
- primary/shared control loss marks all child handles closed and wakes waiters;
- one child failure does not kill siblings unless bridge owner loss occurred;
- queue full returns backpressure/typed error; it does not allocate indefinitely;
- receive cancellation does not drop/duplicate another consumer's frame;
- no implicit identity-changing reconnect.

## 9. Compatibility and migration

M003 extends M002 API without breaking ordinary STREAM semantics.

Compatibility profiles may contain router/version hints, but public code must continue to
work with Unknown capabilities and explicit unsupported errors.

If i2pr's SAM 3.3 server Plan 368 is not closed at M003 execution, i2pr rows may be
recorded as expected unsupported/partial; do not block Java/i2pd correctness on an
unfinished downstream router.

## 10. Required tests

- each datagram style ordinary session;
- payload min/max/max+1;
- local UDP bind restrictions;
- I2P port/protocol preservation;
- authenticated source positive/negative;
- DATAGRAM3 hash cannot convert implicitly to authenticated Destination;
- RAW has no source;
- shared owner create;
- add/remove each child style;
- duplicate ID/listen tuple;
- sibling survives child removal;
- owner close tears all children down;
- cross-owner child attachment impossible;
- concurrent child adds bounded;
- MASTER/PRIMARY dialect behavior;
- capability Unknown/Supported/Unsupported transitions;
- live Java/i2pd/i2pr matrix.

## 11. Required verification commands

M002 floor plus explicit datagram/shared suites and environment-gated live-router matrix.

The closure must include the exact commands chosen by the implementation, and a
machine-readable or durable table with:

```text
router
router_version_or_sha
negotiated_sam_version
feature
dialect
result
notes
```

## 12. Documentation updates

- datagram security model;
- source-authentication table;
- local-vs-I2P ports;
- shared-session lifecycle;
- PRIMARY/MASTER compatibility;
- router compatibility matrix;
- roadmap/registry.

## 13. Acceptance criteria

1. Legacy DATAGRAM and RAW work where router supports them.
2. DATAGRAM2/3 work or return precise unsupported capability without lying.
3. Source trust is type/semantic enforced.
4. Shared session preserves one Destination across STREAM/datagram children.
5. Child add/remove and owner teardown are deterministic.
6. Auto dialect is evidence-based and cannot create duplicate leaked owners.
7. Java I2P matrix is executed.
8. i2pd matrix is executed.
9. i2pr matrix is executed or explicitly records unfinished upstream support.
10. No capability is marked supported solely because HELLO negotiated 3.3.

## 14. Stop conditions

Stop if:

- automatic PRIMARY/MASTER fallback can leave an ambiguous live session;
- router implementations require semantically different shared-session ownership rather
  than a narrow syntax/quirk adapter;
- Datagram2/3 source semantics in deployed routers contradict the current spec materially;
- supporting a datagram mode requires unbounded queues or leaking host-network authority;
- M002 API cannot be extended without breaking its core ownership model.

## 15. Closure evidence required

Exact router pins, full feature/dialect matrix, same-Destination hashes for shared child
tests, queue/bounds results, source-authentication negative tests, teardown/cancellation
tests, and all verification commands actually run.

## 16. Handoff notes

This is the foundation's most interoperability-sensitive milestone. Prefer explicit
Unsupported/Unknown over optimistic feature guessing. M004 should receive a behaviorally
complete client, not a list of partially implemented 3.3 tokens.

Closure evidence: `plans/closure/sam-library/003-status.md`. The Java/i2pd/i2pr live matrix
and control-socket datagram modes remain explicit residual conditions.
