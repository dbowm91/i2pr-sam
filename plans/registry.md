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
| SAM library foundation | active corrective line | `plans/subsystems/sam-library-roadmap.md` | M005 ready; M006 blocked on M005 | M001 closed; M002–M004 remain conditionally closed. M005 closes live protocol/interoperability truth; M006 closes verification/CI/API-quality debt before downstream bindings/adapter work. |

## Dependency-ready implementation plans

| Subsystem | Milestone | State | Handoff | Dependencies |
|---|---:|---|---|---|
| SAM library | 005 | **ready** | `plans/implementation/sam-library/005-foundation-live-interoperability-and-protocol-closure-corrective.md` | none beyond existing M001–M004 implementation baseline |

## Closed implementation plans

| Subsystem | Milestone | State | Handoff | Blocker |
|---|---:|---|---|---|
| SAM library | 001 | closed | `plans/implementation/sam-library/001-clean-room-protocol-capability-foundation.md` | — |
| SAM library | 002 | conditionally closed | `plans/implementation/sam-library/002-async-client-stream-foundation.md` | Live router qualification |
| SAM library | 003 | conditionally closed | `plans/implementation/sam-library/003-datagram-shared-session-interoperability.md` | Live router qualification and control-socket datagram modes |
| SAM library | 004 | conditionally closed | `plans/implementation/sam-library/004-runtime-facades-conformance-api-stabilization.md` | Live router qualification and broader recovery evidence |

Closure evidence is under `plans/closure/sam-library/`.

## Blocked registered corrective plans

| Subsystem | Milestone | State | Handoff | Blocker |
|---|---:|---|---|---|
| SAM library | 006 | blocked | `plans/implementation/sam-library/006-foundation-verification-ci-api-stabilization-corrective.md` | M005 closure |

## Future roadmap-only work

The canonical phase ordering is unchanged, but subsystem-local future milestone numbers
shifted to make room for the two corrective gates discovered by M002–M004 closure review.

- M007 C ABI + Python bindings — blocked on strict M006 foundation closure. Repository
  license selection is still required before public package distribution.
- M008 i2pr service-tunnel adapter — blocked on M006 plus a stable merged
  `i2pr-service-tunnels` integration revision. i2pr Plan 379 has closed on its work
  branch with the current public consumer contract, but this repository must consume a
  stable selected revision at implementation time.
- M009 tunnel daemon/config/persistence/management API — depends on M008.
- M010 CLI/WebUI/sidecar packaging — depends on M009.

## Active sequence

`sam_library_foundation = 001(closed) -> 002(conditionally closed) -> 003(conditionally closed) -> 004(conditionally closed) -> 005(ready corrective) -> 006(blocked corrective)`

Bindings and the service-tunnel adapter are not implementation-ready until M006 closes.
Protocol/version capability claims continue to require live evidence rather than inference
from registration or negotiated version alone.
