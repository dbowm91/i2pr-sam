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
| SAM library foundation | active | `plans/subsystems/sam-library-roadmap.md` | M001 ready; M002–M004 blocked in sequence | M001 has no hard dependency. M002 waits on M001; M003 on M002; M004 on M003. |

## Dependency-ready implementation plans

| Subsystem | Milestone | State | Handoff | Dependencies |
|---|---:|---|---|---|
| SAM library | 001 | **ready** | `plans/implementation/sam-library/001-clean-room-protocol-capability-foundation.md` | none |

## Blocked registered plans

| Subsystem | Milestone | State | Handoff | Blocker |
|---|---:|---|---|---|
| SAM library | 002 | blocked | `plans/implementation/sam-library/002-async-client-stream-foundation.md` | M001 closure |
| SAM library | 003 | blocked | `plans/implementation/sam-library/003-datagram-shared-session-interoperability.md` | M002 closure |
| SAM library | 004 | blocked | `plans/implementation/sam-library/004-runtime-facades-conformance-api-stabilization.md` | M003 closure |

## Future roadmap-only work

- M005 C ABI + Python bindings — plan only after M004 API stabilization.
- M006 i2pr service-tunnel adapter — plan only after M004 and a current stable
  `i2pr-service-tunnels` integration revision; i2pr Plan 379 is reconciling the
  current license/package/consumer handoff.
- M007 tunnel daemon/config/persistence/management API — depends on M006.
- M008 CLI/WebUI/sidecar packaging — depends on M007.

## Active sequence

`sam_library_foundation = 001(ready) -> 002(blocked) -> 003(blocked) -> 004(blocked)`

No SAM protocol capability is implemented or advertised by registration alone.
