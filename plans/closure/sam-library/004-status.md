# SAM Library M004 closure

Status: **conditionally closed**.

Implementation SHA: `956238fcced742836833befaf6e748e97f435001`.

## Evidence

- `i2pr-sam-blocking` wraps the canonical async client. STREAM, datagram, and shared-child
  operations use the same async implementation. Blocking calls made from a Tokio runtime
  return `NestedRuntime`; blocking stream and datagram receives have explicit timeouts.
- `ReconnectPolicy` bounds initial bridge connection attempts and total elapsed time,
  doubles backoff up to a configured ceiling, and retries only transport-transient
  failures. Protocol/configuration errors and user close are not retried. It does not
  reconnect an existing session or recreate an identity.
- Deterministic mock suites cover malformed/oversized replies, timeouts, stream data
  handoff, owner lifecycle, and 100 stream session create/use/close resource cycles.
- `sam-conformance` emits structured JSON with router/version label, negotiated SAM
  version, and truthful probe outcomes. Probe success means session/child create and close,
  not payload exchange. Schema: `specs/conformance.schema.json`.
- `scripts/check-api-snapshot.py` captures public declaration signatures, fields, and enum
  variants for all three crates. The snapshot positive control passed.
- Blocking STREAM parity passed against the deterministic bridge; nested-runtime rejection
  passed.

## Commands executed

The stable and Rust 1.89 check, test, and Clippy commands listed in M001 closure passed.
`rtk cargo fmt --all --check`, warning-free workspace docs, `rtk cargo tree -p
i2pr-sam-proto`, `rtk python3 scripts/check-proto-boundary.py`,
`rtk python3 scripts/check-api-snapshot.py`, and JSON schema parsing all passed. The final
stable test run reported 18 passed.

## Conformance attempts

The runner was invoked against each pinned router label at `127.0.0.1:7656`. All exited 2
with `Connection refused`; exact commands and environment evidence are in
`specs/live-router-qualification.md`. No live feature row is a pass.

## Deviations and residual risk

The public API baseline is a declaration snapshot, not a semver tool. Recovery is limited
to creating a client connection; existing sessions are not automatically recovered.
There is no global concurrent-recovery limiter, deterministic clock abstraction, live
router matrix, shared-session payload soak, or external resource-count measurement. M004's
live qualification acceptance remains outstanding, so closure is conditional.

## Successor audit

M005 C/Python bindings may now be planned against the captured API baseline, but no binding
handoff has been registered and package publication remains blocked by the unselected
repository license. M006 remains blocked on a current stable i2pr service-tunnel integration
contract and the separately tracked i2pr Plan 379 handoff. M007 and M008 remain blocked on
their upstream dependencies.
