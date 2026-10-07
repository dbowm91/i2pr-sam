# i2pr-sam Long-Term Roadmap

Status: canonical dependency ordering; detailed execution belongs in subsystem roadmaps.

## Phase 1 — protocol/client foundation

1. Clean-room reference freeze, bounded protocol/state machine, and capability model.
2. Async client foundation: HELLO, naming, Destination lifecycle, STREAM.
3. Datagram families and shared-Destination sessions with deployed-router interop.
4. Blocking facade, resilience semantics, deterministic bridge harness, and public API
   stabilization.

This phase must complete before bindings or a tunnel daemon become release surfaces.

## Phase 2 — language/application integration

5. Stable C ABI and Python bindings over the proven Rust API.
6. SAM-backed service-tunnel adapter consuming `i2pr-service-tunnels`.

The service-tunnel adapter depends on the i2pr portable-core handoff remaining public and
runtime-neutral. If the adapter discovers a missing transport-neutral seam, fix it in i2pr
with a failing external fixture rather than duplicating policy here.

## Phase 3 — standalone tunnel manager

7. Headless daemon/config/persistence/reload/management API.
8. CLI and self-contained WebUI clients over the daemon API.
9. App-sidecar packaging and portable distribution.

## Phase 4 — maturity

10. Multi-router compatibility hardening and long-running recovery testing.
11. Performance/resource-budget profiling.
12. Release/publication hardening and stable compatibility policy.

## Dependency rules

- Python/C must not define semantics absent from Rust.
- The daemon must not become the only usable form of the library.
- WebUI/CLI must not own tunnel policy.
- Router compatibility aliases stay in the SAM compatibility layer.
- No later phase weakens source-authentication typing or clean-room provenance.
