# 011 — Live router payload qualification

Class: corrective qualification + infrastructure.

Status: **active — Java I2P same-router STREAM request/response verified; remaining feature matrix is open**.

## Objective

Produce live payload evidence against a real I2P service so milestone 005's missing live
evidence can be reconciled to a strict post-corrective closure. A service tunnel hosted by
the tested router is a valid peer for this purpose; a second router implementation is not
a prerequisite.

## Validation basis amendment — 2026-10-08

The user clarified that two-router evidence is unnecessary when a SAM client can reach a
valid I2P service on the same router. The successful Java I2P SAM request below demonstrates
that path. This amended M011 basis supersedes historical multi-router wording in earlier
roadmap and closure records; those records remain unchanged as history.

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

On 2026-10-08, after the user authorized router installation, Java I2P and the pinned i2pd
source build were installed and both SAM bridges passed the corrected HELLO preflight.
The full matrix reached both routers but no payload row ran: Java I2P produced nine
`not_run` rows because local/peer identity lookup returned no concrete Destination; i2pd
produced eight `not_run` rows for the same peer-identity blocker, plus an explicit
unsupported PRIMARY shared dialect. i2pd's router log also reported that it had no peers
available for inbound tunnel creation. The UDP probe observed a reply at DNS/53 and left
high-port reachability unknown. These facts do not establish an egress policy or a router
payload incompatibility. Artifacts and the exact attempt details are recorded under
`artifacts/interop/m011-2026-10-08/` and `specs/live-router-qualification.md`.

A follow-up Java-only SAM HTTP probe resolved the Java router's active webserver tunnel
Destination and received `HTTP/1.1 200 OK` with 1,186 response bytes. The reproducible
`sam-http-get` example and exact output are recorded in
`artifacts/interop/m011-2026-10-08/attempt.md`. The first same-router attempt had run before
that service tunnel was active; it is superseded by this successful service-backed request.

A subsequent pinned-i2pd same-router attempt configured a temporary UDP echo server tunnel
and pointed both harness endpoints at the same SAM bridge. The runner still produced no
payload rows because it requires a transient peer identity before each exchange; the
separate direct datagram echo probe timed out. The schema-valid artifact and matrix are
preserved under `artifacts/interop/m011-2026-10-08/`. This narrows the remaining work: the
harness must accept a known service Destination for payload tests, and DATAGRAM/RAW need a
working echo receiver while shared-session validation needs a SAM peer that can observe
subsession identity. These rows remain open; no cross-router tunnel is required or claimed.

## Invariants

- a `pass` row requires exact payload agreement in both directions;
- shared-session rows require a concrete Destination hash and per-child identity proof;
- DATAGRAM3 source remains an unverified hash and RAW carries no source identity;
- private Destination material never reaches an artifact;
- an unprovisioned environment yields `not_run` rows naming the blocker, never `unsupported`
  or `fail`.

## In scope

- provisioning one router with a reachable SAM bridge and an addressable I2P service;
- running live payload operations through that router, including a STREAM request to a
  valid I2P HTTP service (which may be hosted by the same router);
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

Run these commands locally or on an operator-provisioned host that can reach a SAM bridge
and the configured service. A same-router probe may set `--peer-endpoint` equal to
`--endpoint`; this tests two SAM clients through one router and does not claim cross-router
compatibility. The focused HTTP probe directly connects to a known service Destination.
M012 removed
the GitHub live workflow because no self-hosted Actions runner is registered and
GitHub-hosted loopback cannot reach operator routers.

## Acceptance criteria

1. Java I2P resolves a concrete I2P service Destination through SAM, sends an HTTP request
   over STREAM, and receives a valid 2xx HTTP response with nonempty payload bytes. The
   router, SAM version, target address, response status, byte count, and command are recorded.
2. D1, D2, D3, and RAW each produce a payload row with the correct trust typing.
3. Both datagram transports are exercised, or an `unsupported` row names the router verdict.
4. A MASTER or PRIMARY shared session passes a subsession payload row with one Destination
   hash observed by both ends.
5. Every conformance artifact validates against the schema, and the matrix reports no `fail` row.
6. The blocker record in `specs/live-router-qualification.md` is replaced by the real
   evidence, and milestone 005/006 closure records are superseded additively.

## Stop conditions

Stop and register a corrective if a router disagrees with the pinned specification text; a
disagreement is a finding to be characterised, not a reason to weaken a row.

## Closure evidence

Artifacts under `artifacts/interop/`, the generated matrix CSV, probe output, router
versions actually exercised, and the exact revision at which each row passed.

## Handoff notes

Run from a host that has provisioned a router and a reachable SAM bridge/service. The peer
endpoint is passed through to `sam-conformance`. UDP preflight is advisory; a
known bridge is attempted regardless of arbitrary UDP probe silence. The focused
`sam-http-get` path resolves the service Destination through the SAM bridge and tests a
real request/response over STREAM. It can use a router's own HTTP server tunnel and does
not require cross-router tunnels. The full matrix still requires payload evidence for each
in-scope feature, but it no longer requires a second router implementation. The current
harness's generated-peer flow cannot use a preconfigured service Destination, so adding a
known-destination payload lane is the next M011 implementation task.
