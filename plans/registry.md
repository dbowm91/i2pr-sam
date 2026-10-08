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
| SAM library foundation | strict foundation closure (M011 closed) | `plans/subsystems/sam-library-roadmap.md` | M011 closed: Java I2P full matrix 11/11 pass (known-service HTTP STREAM + same-router STREAM/DATAGRAM/RAW/D2/D3/shared); workspace + harness suites green | M001 closed; M002–M006 retain historical conditional closure, residual live-payload condition satisfied by M011. M012–M016 are closed. No remaining foundation gate. |

## Dependency-ready implementation plans

(none — all implementation plans are closed; future work is roadmap-only.)

## Closed implementation plans

| Subsystem | Milestone | State | Handoff | Blocker |
|---|---:|---|---|---|
| SAM library | 001 | closed | `plans/implementation/sam-library/001-clean-room-protocol-capability-foundation.md` | — |
| SAM library | 002 | conditionally closed | `plans/implementation/sam-library/002-async-client-stream-foundation.md` | Live router qualification |
| SAM library | 003 | conditionally closed | `plans/implementation/sam-library/003-datagram-shared-session-interoperability.md` | Live router qualification and control-socket datagram modes |
| SAM library | 004 | conditionally closed | `plans/implementation/sam-library/004-runtime-facades-conformance-api-stabilization.md` | Live router qualification and broader recovery evidence |
| SAM library | 005 | conditionally closed | `plans/implementation/sam-library/005-foundation-live-interoperability-and-protocol-closure-corrective.md` | Live payload evidence — satisfied by M011 closure under the amended single-router basis |
| SAM library | 006 | conditionally closed | `plans/implementation/sam-library/006-foundation-verification-ci-api-stabilization-corrective.md` | M005 strict closure, i.e. M011 (now closed) |
| SAM library | 011 | closed | `plans/implementation/sam-library/011-live-router-payload-qualification.md` | `plans/closure/sam-library/011-status.md` |
| SAM library | 012 | closed | `plans/implementation/sam-library/012-live-qualification-harness-correctness-corrective.md` | — |
| SAM library | 013 | closed | `plans/implementation/sam-library/013-blocking-parity-and-foundation-closure-truth-corrective.md` | — |
| SAM library | 014 | closed | `plans/implementation/sam-library/014-sam-bridge-preflight-greeting-corrective.md` | — |
| SAM library | 015 | closed | `plans/implementation/sam-library/015-conformance-datagram2-runner-dispatch-corrective.md` | — |
| SAM library | 016 | closed | `plans/implementation/sam-library/016-i2pd-unknown-style-verdict-corrective.md` | — |

Closure evidence is under `plans/closure/sam-library/`.

## Future roadmap-only work

The canonical phase ordering is unchanged, but subsystem-local future milestone numbers
shifted to make room for the two corrective gates discovered by M002–M004 closure review.

- M007 C ABI + Python bindings — M011 strict live closure and M013 blocking-parity closure are both satisfied; remaining blockers: repository license selection before public package distribution.
- M008 i2pr service-tunnel adapter — M011 is closed and M013 is closed; remaining blocker: a stable merged
  `i2pr-service-tunnels` integration revision. i2pr Plan 379 has closed on its work
  branch with the current public consumer contract, but this repository must consume a
  stable selected revision at implementation time.
- M009 tunnel daemon/config/persistence/management API — depends on M008.
- M010 CLI/WebUI/sidecar packaging — depends on M009.
- M012 live qualification harness correctness — closed; peer-endpoint flow, advisory UDP semantics, deterministic regression checks, and Python-artifact hygiene are corrected. The live Actions workflow was removed because no self-hosted runner is registered.
- M013 blocking parity and closure truth — closed; its additive record supplies the DATAGRAM/RAW/D2/D3/shared/timeout/runtime parity evidence missing from M006.
- M014 SAM bridge preflight greeting — closed; the harness now sends the SAM v3 HELLO grammar accepted by Java I2P and i2pd.
- M015 conformance DATAGRAM2 dispatch — closed; DATAGRAM2/DATAGRAM3 rows no longer panic the runner.
- M016 i2pd unknown-style verdict — closed; its explicit unknown-style response is recorded as unsupported.

## Active sequence

`sam_library_foundation = 001(closed) -> 002(conditionally closed) -> 003(conditionally closed) -> 004(conditionally closed) -> 005(conditionally closed) -> 006(conditionally closed) -> {012(closed), 013(closed), 014(closed), 015(closed), 016(closed)} -> 011(closed: Java I2P 11/11 live payload pass) -> strict foundation closure`

Bindings and the service-tunnel adapter remain not implementation-ready. M012 repaired M011's peer-endpoint path, workflow topology, UDP readiness semantics, and Python artifact hygiene. M013 independently supplied the missing blocking-parity evidence. M014–M016 repaired defects found during live qualification. M011 closed the foundation gate: the known-service HTTP STREAM lane passes with response status/body evidence, and same-router sender/receiver sessions pass every datagram and shared child row. A second router implementation, external datagram service, or cross-router tunnel is not required by the amended M011 basis.
