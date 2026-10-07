# SAM Library Roadmap

Status: active — M005 foundation interoperability corrective ready; M006 verification/API corrective blocked on M005.

Long-term references:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`

Related ADRs:

- `plans/adrs/ADR-0001-clean-room-reference-and-provenance-policy.md`
- `plans/adrs/ADR-0002-layered-client-runtime-and-bindings-ownership.md`

External contracts:

- official SAM v3 documentation: <https://geti2p.net/en/docs/api/samv3>
- i2pr portable adapter handoff:
  <https://github.com/dbowm91/i2pr/blob/main/specs/references/portable-service-tunnel-sam-adapter-handoff.md>

## 1. Purpose and ownership boundary

This workstream owns the reusable client-side SAM implementation: protocol/state,
connections, sessions, router compatibility, Rust API, and the foundational conformance
harness.

It does not own an I2P router, I2CP implementation, I2P Streaming implementation, or
service-profile/privacy policy already owned by i2pr's portable service-tunnel core.

## 2. Work classification

### Invariants

- clean-room provenance;
- bounded parsing/allocation/queues;
- typed source-authentication semantics;
- explicit control-to-data STREAM transition;
- capability model distinct from negotiated version;
- exact close/cancellation ownership;
- no secret leakage in logs/errors.

### Capabilities

- Destination/naming;
- STREAM;
- DATAGRAM/RAW/DATAGRAM2/DATAGRAM3;
- shared-Destination child sessions;
- blocking facade after async semantics are proven.

### Infrastructure

- codec/state-machine crate;
- async transport/session machinery;
- router-profile/capability layer;
- deterministic mock bridge;
- external conformance harness.

### Polish

- ergonomic builders;
- diagnostics;
- examples;
- packaging/publication.

## 3. Non-goals

Initial foundation milestones do not:

- implement a SAM server;
- implement I2CP/Streaming internally;
- publish crates;
- ship Python/C bindings;
- ship a daemon/WebUI;
- consume i2pr service-tunnel policy yet;
- assume SAM 3.3 version negotiation means all 3.3 features work;
- hide DATAGRAM3's unauthenticated source behind an authenticated address type.

## 4. Current state

M001 is strictly closed. M002–M004 landed substantial implementation at
`956238fcced742836833befaf6e748e97f435001` and are conditionally closed.

Implemented today:

- runtime-neutral bounded protocol/state crate;
- Tokio async HELLO, Destination generation, naming, ordinary STREAM;
- UDP-forwarded DATAGRAM/RAW/DATAGRAM2/DATAGRAM3 surfaces;
- explicit MASTER/PRIMARY shared owner plus child add/remove;
- blocking facade;
- bounded initial bridge-connect retry;
- mock/fault/lifecycle tests;
- conformance executable and public-API snapshot.

The conditional closures are material, not administrative. No Java I2P, i2pd, or i2pr
live payload row has passed yet; every attempted bridge connection was refused because no
router was present in the implementation environment. The shared-session mock compares
the stored request token `TRANSIENT`, not a concrete router-generated Destination.
Ordinary v1/v2-compatible DATAGRAM/RAW control-socket modes planned by M003 are absent.
Blocking parity is incomplete, resource qualification is narrow, and no hosted CI exists.

Therefore M005/M006 are corrective prerequisites before bindings or the portable
service-tunnel adapter can become implementation-ready.

## 5. Target architecture

Initial workspace target:

```text
crates/i2pr-sam-proto
  bounded command/reply codecs + state legality
            |
            v
crates/i2pr-sam
  canonical async client
  capabilities + destination/naming + sessions
            |
            +--> later blocking / C / Python
            |
            +--> later service-tunnel adapter
                       |
                       v
                    daemon
```

The exact later crate split is not frozen until those milestones are planned.

## 6. Dependency graph

```text
001 protocol/reference/capability foundation
  |
  v
002 async client + Destination/NAMING/STREAM
  |
  v
003 datagram families + shared sessions + live router interop
  |
  v
004 blocking facade + resilience + conformance/public API stabilization
  |
  v
005 live interoperability + protocol-closure corrective
  |
  v
006 verification/CI/public-API stabilization corrective
  |
  +--> 007 foreign-language bindings            [future]
  |
  +--> 008 service-tunnel adapter               [future; i2pr contract]
          |
          v
        009 daemon/config/management             [future]
          |
          v
        010 CLI/WebUI/sidecar packaging          [future]
```

M005 corrects the conditional wire/interoperability authority left by M002–M004. M006 is a
hard dependency on M005 because API stabilization must follow live wire truth.

The i2pr portable-core API is an interface dependency for future M008, not for the base
SAM client. The canonical long-term phase ordering is unchanged; only subsystem-local
milestone numbers for future work move because two corrective gates were inserted.

## 7. Milestones

### 001 — Clean-room protocol and capability foundation

Class: invariant + infrastructure.

Objective: freeze references; create the Rust workspace/protocol core; implement bounded
SAM line syntax, typed command/reply vocabulary, version negotiation model, state legality,
and a capability representation that does not equate version with feature support.

Dependencies: none.

Exit: protocol crate is socket-free; normative/reference freeze exists; deterministic
codec/state tests and negative bounds tests pass.

### 002 — Async client, Destination/NAMING, and STREAM foundation

Class: capability + infrastructure.

Objective: establish the canonical async Rust API and complete ordinary STREAM client
semantics over real SAM bridges.

Dependency: 001.

Exit: HELLO, DEST, NAMING, ordinary STREAM CREATE/CONNECT/ACCEPT and close/cancel semantics
work against deterministic harness plus at least Java I2P and one second router.

### 003 — Datagram and shared-Destination interoperability

Class: capability + invariant.

Objective: complete DATAGRAM, RAW, DATAGRAM2, DATAGRAM3 and shared PRIMARY/MASTER
subsessions while preserving source-authentication distinctions.

Dependency: 002.

Exit: live Java/i2pd/i2pr matrix is recorded; shared STREAM/datagram children preserve one
Destination; dialect fallback is evidence-driven and centralized.

### 004 — Runtime facades, resilience, conformance, and API stabilization

Class: infrastructure + polish + invariant.

Objective: add blocking facade, bounded retry/reconnect behavior, deterministic bridge
fault harness, long-running lifecycle tests, and the first reviewed public API baseline.

Dependency: 003.

Exit: foundation API is stable enough to design language bindings and the service-tunnel
adapter without exposing protocol strings or runtime internals.

### 005 — Live interoperability and protocol-closure corrective

Class: capability + invariant + corrective qualification.

Objective: close the live Java/i2pd/i2pr protocol evidence, concrete shared-Destination
identity proof, missing ordinary control-socket D1/RAW modes, and conformance-runner
truthfulness gaps carried by M002–M004.

Dependency: 004 conditional implementation.

Exit: live payload matrix and concrete same-Destination evidence satisfy the corrective
acceptance criteria in
`plans/implementation/sam-library/005-foundation-live-interoperability-and-protocol-closure-corrective.md`.

### 006 — Verification, CI, and public-API stabilization corrective

Class: invariant + infrastructure + polish.

Objective: close blocking parity, cancellation/contention, retry semantics, resource
soak, semantic API, guard-mutation, and hosted-CI gaps before downstream API consumers
freeze the Rust surface.

Dependency: 005.

Exit: M002–M004 conditional foundation authority is reconciled additively to a strict
post-corrective foundation closure.

### 007 — C ABI and Python bindings

Future, unplanned implementation handoff. Blocked on M006.

### 008 — i2pr service-tunnel adapter

Future, unplanned implementation handoff. Blocked on M006 plus a stable merged
`i2pr-service-tunnels` integration revision; consumes the public policy/filter core and
does not copy it.

### 009 — Tunnel daemon and management API

Future, unplanned implementation handoff.

### 010 — CLI/WebUI/sidecar packaging

Future, unplanned implementation handoff.

## 8. Cross-cutting requirements

### Protocol and compatibility

- preserve exact negotiated syntax rules;
- track optional capability support independently;
- centralize PRIMARY/MASTER and router quirks;
- reject unknown/unsupported operations deterministically.

### Security

- named parser/frame/session/queue limits;
- redacted secret-bearing types;
- no peer-controlled panic;
- DATAGRAM3 source is an unverified hash;
- RAW has no source identity;
- optional authentication/TLS never silently downgrades when requested.

### Concurrency/cancellation/recovery

- all owned tasks have parents and cancellation;
- no detached session tasks;
- close is idempotent at the semantic layer;
- retry/backoff is bounded and cancellation-aware;
- shared-session owner loss deterministically invalidates children.

### Performance/resource use

- no unbounded read-until-newline;
- no unbounded maps by router/session ID;
- payload limits follow protocol constraints and local policy ceilings;
- backpressure precedes memory growth.

## 9. Verification strategy

Use three evidence layers:

1. pure protocol/property/negative tests;
2. deterministic fake-bridge state/lifecycle/fault tests;
3. live reference-router interoperability.

The live matrix records router/version, negotiated SAM version, capability exercised,
wire dialect, result, and known deviation.

## 10. Risks and decision points

- official documentation may lag deployed Datagram2/3 implementation status;
- PRIMARY/MASTER compatibility is a semantic split under one nominal SAM version;
- over-generalizing runtime support before the core API stabilizes could create needless
  generic complexity;
- datagram local-UDP forwarding is unsuitable for some embedded/private transports, so
  the API must not assume it is the only receive model;
- a high-level API that hides authentication differences could create application
  security bugs.

## 11. Completion definition

The initial implementation line 001–004 produced one strict closure and three conditional
closures. Foundation closure now additionally requires M005 and M006.

Strict foundation closure requires live multi-router payload evidence, concrete
shared-Destination identity, truthful datagram mode coverage, blocking parity,
cancellation/contention/resource evidence, semantic API review, guard self-tests, and
hosted cross-platform/MSRV CI.

Bindings, service-tunnel adapter, tunnel manager, and UI are not required for foundation
closure and must not begin implementation before M006 closes.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure | Blocker |
|---|---|---|---|---|
| 001 | closed | `plans/implementation/sam-library/001-clean-room-protocol-capability-foundation.md` | `plans/closure/sam-library/001-status.md` | — |
| 002 | conditionally closed | `plans/implementation/sam-library/002-async-client-stream-foundation.md` | `plans/closure/sam-library/002-status.md` | corrected by 005/006 |
| 003 | conditionally closed | `plans/implementation/sam-library/003-datagram-shared-session-interoperability.md` | `plans/closure/sam-library/003-status.md` | corrected by 005/006 |
| 004 | conditionally closed | `plans/implementation/sam-library/004-runtime-facades-conformance-api-stabilization.md` | `plans/closure/sam-library/004-status.md` | corrected by 005/006 |
| 005 | ready | `plans/implementation/sam-library/005-foundation-live-interoperability-and-protocol-closure-corrective.md` | — | — |
| 006 | blocked | `plans/implementation/sam-library/006-foundation-verification-ci-api-stabilization-corrective.md` | — | 005 closure |
| 007 | proposed | — | — | 006 |
| 008 | proposed | — | — | 006 + stable merged i2pr portable-core revision |
| 009 | proposed | — | — | 008 |
| 010 | proposed | — | — | 009 |
