# i2pr-sam Active Planning Registry

This is the compact control surface for current interim planning. Detailed requirements
live in canonical documents, ADRs, subsystem roadmaps, implementation plans, and closure
records.

Canonical direction:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`
- `plans/003-planning-process.md`

## Status vocabulary

- **proposed** — direction exists but no execution handoff is ready.
- **ready** — dependencies/interfaces satisfied; may be handed off.
- **active** — implementation/closure work in progress.
- **blocked** — named dependency/evidence prevents execution.
- **closing** — implementation landed; closure evidence is being assembled.
- **closed** — closure accepted.
- **conditionally closed** — substantial work landed with named residual condition.
- **superseded** — replaced by a later record.
- **archived** — inactive history retained for traceability.

## Active subsystem roadmaps

| Subsystem | Status | Roadmap | Current milestone | Dependencies/blockers |
|---|---|---|---|---|
| SAM library foundation | active live qualification | `plans/subsystems/sam-library-roadmap.md` | M011 blocked on suitable router provisioning | M001 closed; M002–M006 retain historical conditional closure. M012 repaired qualification machinery; M013 supplied the missing blocking parity evidence. M011 is now the sole remaining foundation gate. |

## Dependency-ready implementation plans

| Subsystem | Milestone | State | Handoff | Dependencies |
|---|---:|---|---|---|
| SAM library | 012 | **ready** | `plans/implementation/sam-library/012-live-qualification-harness-correctness-corrective.md` | none |
| SAM library | 013 | **ready** | `plans/implementation/sam-library/013-blocking-parity-and-foundation-closure-truth-corrective.md` | none; may execute in parallel with 012 |

## Closed implementation plans

| Subsystem | Milestone | State | Handoff | Blocker |
|---|---:|---|---|---|
| SAM library | 001 | closed | `plans/implementation/sam-library/001-clean-room-protocol-capability-foundation.md` | — |
| SAM library | 002 | conditionally closed | `plans/implementation/sam-library/002-async-client-stream-foundation.md` | Live router qualification |
| SAM library | 003 | conditionally closed | `plans/implementation/sam-library/003-datagram-shared-session-interoperability.md` | Live router qualification and control-socket datagram modes |
| SAM library | 004 | conditionally closed | `plans/implementation/sam-library/004-runtime-facades-conformance-api-stabilization.md` | Live router qualification and broader recovery evidence |
| SAM library | 005 | conditionally closed | `plans/implementation/sam-library/005-foundation-live-interoperability-and-protocol-closure-corrective.md` | Live multi-router payload matrix; corrected in M011 |
| SAM library | 006 | conditionally closed | `plans/implementation/sam-library/006-foundation-verification-ci-api-stabilization-corrective.md` | M005 strict closure, i.e. M011 |
| SAM library | 012 | closed | `plans/implementation/sam-library/012-live-qualification-harness-correctness-corrective.md` | — |
| SAM library | 013 | closed | `plans/implementation/sam-library/013-blocking-parity-and-foundation-closure-truth-corrective.md` | — |

Closure evidence is under `plans/closure/sam-library/`.

## Blocked registered corrective plans

| Subsystem | Milestone | State | Handoff | Blocker |
|---|---:|---|---|---|
| SAM library | 011 | blocked | `plans/implementation/sam-library/011-live-router-payload-qualification.md` | Suitable two-router provisioning and reachable SAM bridges |

## Future roadmap-only work

The canonical phase ordering is unchanged, but subsystem-local future milestone numbers
shifted to make room for the two corrective gates discovered by M002–M004 closure review.

- M007 C ABI + Python bindings — blocked on M011 strict live closure **and** M013 blocking-parity closure. Repository license selection is still required before public package distribution.
- M008 i2pr service-tunnel adapter — blocked on M011 + M013 plus a stable merged
  `i2pr-service-tunnels` integration revision. i2pr Plan 379 has closed on its work
  branch with the current public consumer contract, but this repository must consume a
  stable selected revision at implementation time.
- M009 tunnel daemon/config/persistence/management API — depends on M008.
- M010 CLI/WebUI/sidecar packaging — depends on M009.
- M011 live router payload qualification — blocked only on suitable two-router provisioning; M012 and M013 are closed.
- M012 live qualification harness correctness — closed; peer-endpoint flow, advisory UDP semantics, self-hosted workflow topology, regression checks, and Python-artifact hygiene are corrected.
- M013 blocking parity and closure truth — closed; its additive record supplies the DATAGRAM/RAW/D2/D3/shared/timeout/runtime parity evidence missing from M006.

## Active sequence

`sam_library_foundation = 001(closed) -> 002(conditionally closed) -> 003(conditionally closed) -> 004(conditionally closed) -> 005(conditionally closed) -> 006(conditionally closed) -> {012(closed), 013(closed)} -> 011(blocked on live router provisioning) -> strict foundation closure`

Bindings and the service-tunnel adapter remain not implementation-ready. M012 repaired M011's peer-endpoint path, workflow topology, UDP readiness semantics, and Python artifact hygiene. M013 independently supplied the missing blocking-parity evidence. M011 is now the sole remaining foundation gate and is blocked on suitable two-router provisioning.
