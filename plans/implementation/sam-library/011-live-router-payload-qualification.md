# 011 — Live router payload qualification

Class: corrective qualification + infrastructure.

Status: **ready on provisioning**; blocked only on a router-enabled host.

## Objective

Produce the live multi-router payload matrix that milestone 005 could not run, so the
foundation's conditional closures can be reconciled to a strict post-corrective closure.

Milestone 005 corrected the protocol behaviour and reporting machinery but could not
execute a single live payload row. The implementation host has no provisioned router or
reachable SAM bridge. The earlier UDP probe observed replies only on port 53, but its
silence at arbitrary endpoints cannot establish an egress policy. UDP probe results are
advisory; the live runner tests the actual configured router and peer topology.

## Why ready

- `scripts/interop/qualify.py` provisions, probes, runs, validates, and merges artifacts
  for Java I2P, i2pd, and i2pr, and exits `3` with a named diagnostic category when a lane
  cannot start.
- `sam-conformance` performs the real semantic operations: bidirectional stream payload with
  non-silent accept peer capture, D1/D2/D3/RAW payload exchange over both transports, and
  shared-session subsession exchange with per-child same-Destination proof.
- `scripts/check-conformance-artifact.py` rejects any artifact claiming a pass without
  payload evidence.
- `scripts/interop/udp_egress_probe.py` reports observed UDP replies or unknown. It is
  advisory and does not prevent attempting a known SAM bridge.

## Current evidence

Three router pins verified by `git ls-remote` on 2026-10-07 (see
`specs/references/sam-v3-reference-freeze.md`). Harness artifacts for all three routers are
`not_run` with `capability_passes=0`; see `specs/live-router-qualification.md`.

## Invariants

- a `pass` row requires exact payload agreement in both directions;
- shared-session rows require a concrete Destination hash and per-child identity proof;
- DATAGRAM3 source remains an unverified hash and RAW carries no source identity;
- private Destination material never reaches an artifact;
- an unprovisioned environment yields `not_run` rows naming the blocker, never `unsupported`
  or `fail`.

## In scope

- provisioning two routers able to reach each other over the I2P network;
- running the full matrix for Java I2P plus at least one second router;
- recording negotiated SAM version, dialect, and payload evidence per row;
- reconciling the M002-M004 conditional authority additively to strict closure.

## Out of scope

- router implementation, I2CP, I2P Streaming;
- new protocol features not already implemented;
- bindings (007) and the i2pr service-tunnel adapter (008), which remain gated on M006.

## Required tests

The matrix itself, executed through the harness. Each row must be `pass` or carry a
defect registered as a new corrective plan rather than being softened in this one.

## Verification commands

```bash
python3 scripts/interop/udp_egress_probe.py
python3 scripts/interop/qualify.py --all --peer-endpoint 127.0.0.1:7657
python3 scripts/check-conformance-artifact.py artifacts/interop/<router>-conformance.json
```

The GitHub Actions `live-interop` workflow runs the same commands manually.

## Acceptance criteria

1. Java I2P passes stream payload in both directions with a peer-observable Destination
   captured on non-silent accept.
2. At least one second router passes the same stream payload row.
3. D1, D2, D3, and RAW each produce a payload row with the correct trust typing.
4. Both datagram transports are exercised, or an `unsupported` row names the router verdict.
5. A MASTER or PRIMARY shared session passes a subsession payload row with one Destination
   hash observed by both ends.
6. Every artifact validates against the schema, and the matrix reports no `fail` row.
7. The blocker record in `specs/live-router-qualification.md` is replaced by the real
   evidence, and milestone 005/006 closure records are superseded additively.

## Stop conditions

Stop and register a corrective if a router disagrees with the pinned specification text; a
disagreement is a finding to be characterised, not a reason to weaken a row.

## Closure evidence

Artifacts under `artifacts/interop/`, the generated matrix CSV, probe output, router
versions actually exercised, and the exact revision at which each row passed.

## Handoff notes

Run from a host that has provisioned routers or can route to both configured SAM bridges.
The peer endpoint is passed through to `sam-conformance`. UDP preflight is advisory; a
known bridge is attempted regardless of arbitrary UDP probe silence. M011 remains blocked
until suitable multi-router provisioning and the required live payload evidence exist.
