# SAM Library Roadmap

Status: active — M005 and M006 retain their historical conditional closures; M012 and M013 are closed; M011 is blocked on suitable live router provisioning.

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

M001 is strictly closed. M002–M006 are conditionally closed. M002–M004 landed substantial
implementation at `956238fcced742836833befaf6e748e97f435001`; M005 and M006 corrected the
protocol and reporting defects those closures named, and closed the verification, API, and
CI debt.

Implemented today:

- runtime-neutral bounded protocol/state crate;
- Tokio async HELLO, Destination generation, naming, ordinary STREAM;
- UDP-forwarded DATAGRAM/RAW/DATAGRAM2/DATAGRAM3 surfaces;
- explicit MASTER/PRIMARY shared owner plus child add/remove;
- blocking facade;
- bounded initial bridge-connect retry;
- ordinary v1/v2-compatible DATAGRAM/RAW control-socket send/receive, bounded and typed;
- concrete shared-session identity (`NAMING LOOKUP NAME=ME` plus a canonical SHA-256 hash)
  with per-subsession identity proof;
- correct non-silent `STREAM ACCEPT` peer-identity framing;
- mock/fault/lifecycle/cancellation/contention tests;
- conformance runner whose rows require payload evidence, plus a schema and validator that
  reject an overclaiming artifact;
- router-enabled interop harness with provisioning probes and a per-router matrix;
- blocking facade parity, typed public API, guard mutation self-tests, process-wide retry
  admission, resource accounting, and hosted Linux/MSRV/macOS/Windows CI.

The conditional closures are still material, and after M005/M006 they are material for one
reason only: **no live Java I2P, i2pd, or i2pr payload row has ever passed.** The
implementation host has no router binary, no container runtime access, and UDP egress
restricted to port 53, which prevents SSU peering on random high ports and therefore
prevents any router here from joining the network. Every protocol claim above is backed by
deterministic mock evidence against the pinned specification, not by a live exchange.

M011 carries that residual. Until it runs, strict foundation closure - and therefore M007
bindings and M008 adapter - stays out of reach by design.

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
  +--> 007 foreign-language bindings            [future; blocked on M011]
  |
  +--> 008 service-tunnel adapter               [future; blocked on M011; i2pr contract]
          |
          v
        009 daemon/config/management             [future]
          |
          v
        010 CLI/WebUI/sidecar packaging          [future]

  011 live router payload qualification   [registered by M005/M006 closure review]
    ^                                     |
    |  strict foundation closure          |  unblocks M007 and M008
    +-------------------------------------|
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

### 011 — Live router payload qualification

Class: corrective qualification + infrastructure.

Objective: produce the live multi-router payload matrix that M005 could not run.

Dependency: **M012 harness-correctness closure plus router provisioning**. M012 repaired the
peer-endpoint path, moved the manual workflow to an explicitly labeled self-hosted topology,
and made UDP preflight advisory because silence from arbitrary endpoints cannot prove an
egress restriction.

Exit: after M012, Java I2P plus at least one second router pass stream payload in both
directions with peer-observable identity, datagram families pass with correct trust typing,
and a shared subsession payload row proves one Destination across children.

### 012 — Live qualification harness correctness corrective

Class: corrective infrastructure + invariant.

Objective: repair peer-endpoint plumbing, make network-readiness probes evidentially sound,
fix the live-workflow topology, add deterministic harness tests, and remove generated
Python bytecode from version control.

Dependency: none beyond the current M005/M006 implementation.

Exit: M011's machinery is executable on a suitable router host and only environment/router
provisioning remains. **Closed**; see `plans/closure/sam-library/012-status.md`.

### 013 — Blocking parity and foundation closure-truth corrective

Class: corrective verification + invariant + polish.

Objective: supply the blocking DATAGRAM/RAW/D2/D3/shared/timeout/runtime parity evidence
that M006 claimed but did not check in, while preserving one canonical async SAM
implementation.

Dependency: none on M012; may execute in parallel.

Exit: the non-live Rust foundation verification is truthful and complete. **Closed**; see
`plans/closure/sam-library/013-status.md`. M011 is the sole remaining foundation gate.

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

M012 corrects the current live-harness readiness defects. M013 corrects the blocking-parity
evidence gap. M011 may become the sole remaining gate only after those corrections close.

Bindings, service-tunnel adapter, tunnel manager, and UI are not required for foundation
closure and must not begin implementation before the corrective/live chain is complete.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure | Blocker |
|---|---|---|---|---|
| 001 | closed | `plans/implementation/sam-library/001-clean-room-protocol-capability-foundation.md` | `plans/closure/sam-library/001-status.md` | — |
| 002 | conditionally closed | `plans/implementation/sam-library/002-async-client-stream-foundation.md` | `plans/closure/sam-library/002-status.md` | corrected by 005/006 |
| 003 | conditionally closed | `plans/implementation/sam-library/003-datagram-shared-session-interoperability.md` | `plans/closure/sam-library/003-status.md` | corrected by 005/006 |
| 004 | conditionally closed | `plans/implementation/sam-library/004-runtime-facades-conformance-api-stabilization.md` | `plans/closure/sam-library/004-status.md` | corrected by 005/006 |
| 005 | conditionally closed | `plans/implementation/sam-library/005-foundation-live-interoperability-and-protocol-closure-corrective.md` | `plans/closure/sam-library/005-status.md` | live payload matrix → 011 |
| 006 | conditionally closed | `plans/implementation/sam-library/006-foundation-verification-ci-api-stabilization-corrective.md` | `plans/closure/sam-library/006-status.md` | 005 strict closure → 011 |
| 007 | proposed | — | — | 011 strict closure + 013 |
| 008 | proposed | — | — | 011 strict closure + 013 + stable merged i2pr portable-core revision |
| 009 | proposed | — | — | 008 |
| 010 | proposed | — | — | 009 |
| 011 | blocked | `plans/implementation/sam-library/011-live-router-payload-qualification.md` | — | suitable two-router provisioning and reachable SAM bridges |
| 012 | closed | `plans/implementation/sam-library/012-live-qualification-harness-correctness-corrective.md` | `plans/closure/sam-library/012-status.md` | — |
| 013 | closed | `plans/implementation/sam-library/013-blocking-parity-and-foundation-closure-truth-corrective.md` | `plans/closure/sam-library/013-status.md` | — |
