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
| SAM library foundation | conditionally closed | `plans/subsystems/sam-library-roadmap.md` | M001 closed; M002–M004 conditionally closed | Live Java I2P/i2pd/i2pr qualification remains open; see closure records. |

## Dependency-ready implementation plans

| Subsystem | Milestone | State | Handoff | Dependencies |
|---|---:|---|---|---|
| — | — | — | — | No active implementation handoff. |

## Closed implementation plans

| Subsystem | Milestone | State | Handoff | Blocker |
|---|---:|---|---|---|
| SAM library | 001 | closed | `plans/implementation/sam-library/001-clean-room-protocol-capability-foundation.md` | — |
| SAM library | 002 | conditionally closed | `plans/implementation/sam-library/002-async-client-stream-foundation.md` | Live router qualification |
| SAM library | 003 | conditionally closed | `plans/implementation/sam-library/003-datagram-shared-session-interoperability.md` | Live router qualification and control-socket datagram modes |
| SAM library | 004 | conditionally closed | `plans/implementation/sam-library/004-runtime-facades-conformance-api-stabilization.md` | Live router qualification and broader recovery evidence |

Closure evidence is under `plans/closure/sam-library/`.

## Future roadmap-only work

- M005 C ABI + Python bindings — API baseline is captured, so creating a bounded M005 plan
  is unblocked. No implementation handoff is registered yet; repository license selection
  is still required before package publication.
- M006 i2pr service-tunnel adapter — plan only after M004 and a current stable
  `i2pr-service-tunnels` integration revision; remains blocked because i2pr Plan 379 is
  reconciling the current license/package/consumer handoff.
- M007 tunnel daemon/config/persistence/management API — depends on M006.
- M008 CLI/WebUI/sidecar packaging — depends on M007.

## Active sequence

`sam_library_foundation = 001(closed) -> 002(conditionally closed) -> 003(conditionally closed) -> 004(conditionally closed)`

Protocol and client capabilities are implemented. Live-router capability claims remain
unqualified and are not inferred from registration or negotiated version alone.
