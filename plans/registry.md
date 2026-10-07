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
| SAM library foundation | corrective line complete; live lane blocked on provisioning | `plans/subsystems/sam-library-roadmap.md` | M011 ready on a router-enabled host | M001 closed; M002–M006 conditionally closed. M005 corrected the protocol and reporting defects; M006 closed verification/API/CI debt. The single remaining gate is live multi-router payload evidence, which M011 carries. |

## Dependency-ready implementation plans

| Subsystem | Milestone | State | Handoff | Dependencies |
|---|---:|---|---|---|
| SAM library | 011 | **ready on provisioning** | `plans/implementation/sam-library/011-live-router-payload-qualification.md` | A host where `scripts/interop/udp_egress_probe.py` reports `i2p_router_udp_capable=true` and two routers can reach each other over the I2P network |

## Closed implementation plans

| Subsystem | Milestone | State | Handoff | Blocker |
|---|---:|---|---|---|
| SAM library | 001 | closed | `plans/implementation/sam-library/001-clean-room-protocol-capability-foundation.md` | — |
| SAM library | 002 | conditionally closed | `plans/implementation/sam-library/002-async-client-stream-foundation.md` | Live router qualification |
| SAM library | 003 | conditionally closed | `plans/implementation/sam-library/003-datagram-shared-session-interoperability.md` | Live router qualification and control-socket datagram modes |
| SAM library | 004 | conditionally closed | `plans/implementation/sam-library/004-runtime-facades-conformance-api-stabilization.md` | Live router qualification and broader recovery evidence |
| SAM library | 005 | conditionally closed | `plans/implementation/sam-library/005-foundation-live-interoperability-and-protocol-closure-corrective.md` | Live multi-router payload matrix; corrected in M011 |
| SAM library | 006 | conditionally closed | `plans/implementation/sam-library/006-foundation-verification-ci-api-stabilization-corrective.md` | M005 strict closure, i.e. M011 |

Closure evidence is under `plans/closure/sam-library/`.

## Blocked registered corrective plans

None. M005 and M006 both executed; neither remains blocked on another milestone.

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
- M011 live router payload qualification — registered by M005/M006 closure review as the
  successor that converts the conditional foundation closures to strict ones. It is the
  single gate standing between the current state and strict M006 closure; M007 and M008
  stay blocked until it runs.

## Active sequence

`sam_library_foundation = 001(closed) -> 002(conditionally closed) -> 003(conditionally closed) -> 004(conditionally closed) -> 005(conditionally closed) -> 006(conditionally closed) -> 011(ready on provisioning)`

Bindings and the service-tunnel adapter remain not implementation-ready. M006 is
conditionally closed rather than closed, so the plan's own dependency rule keeps M007 and
M008 blocked until M011 supplies the live payload matrix and M006 is reconciled additively
to strict closure. Protocol/version capability claims continue to require live evidence
rather than inference from registration or negotiated version alone.
