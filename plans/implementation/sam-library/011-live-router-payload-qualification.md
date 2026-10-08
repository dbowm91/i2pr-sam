# 011 — Live router payload qualification

Class: corrective qualification + infrastructure.

Status: **active — same-router service HTTP was proven directly; timeout and i2pd identity handling are corrected; live payload matrix remains open because this isolated router did not complete a payload exchange**.

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
- `sam-conformance` performs the real semantic operations: known-service HTTP STREAM,
  bidirectional STREAM payload with non-silent accept peer capture, D1/D2/D3/RAW payload
  exchange over supported transports, and shared STREAM/DATAGRAM child payload exchange
  with owner Destination, child removal, and owner teardown checks.
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
payload rows because it resolved a transient peer Destination in a separate session and
then targeted that stale identity; the direct datagram echo probe also timed out. The
schema-valid artifact and matrix are preserved under `artifacts/interop/m011-2026-10-08/`.
The runner has since been corrected to target the Destination of the actual receiving
session. D1/D2/D3/RAW and shared child payload checks use two SAM clients through the same
router; they do not require a separate UDP echo tunnel or another router. Router tunnel
reachability can still prevent a live exchange, in which case the row must remain `not_run`.

The harness now accepts `--service-destination` for the STREAM row. It resolves the supplied
I2P hostname or base32 address through SAM, sends `GET /` over a STREAM session, and records
the target, response status, total bytes, and response-body bytes. It passes only for an HTTP
2xx response with a nonempty body. Its artifact evidence is operation-specific and does not
claim byte-for-byte response echo.

## Full closure evidence still required

The single-router amendment removes cross-router tunnels and a second router implementation
as prerequisites. It does not remove the payload semantics in the M005 corrective. The
remaining closure work is:

| Evidence | Needed setup | Current status |
|---|---|---|
| Java HTTP STREAM request | SAM bridge and reachable HTTP server tunnel; run `qualify.py --plan stream --service-destination ...` | Prior direct probe passed; record a schema-valid artifact using the new lane |
| D1 and RAW over UDP forwarding and control socket | Two SAM clients create transient send and receive sessions through one router; forwarded mode uses local UDP forwarding | Open; runner now targets the receiver session's concrete Destination |
| D2 and D3 payload/trust semantics | Same-router receiver sessions exchange exact payloads; record D2 authenticated and D3 unverified source behavior | Open; runner now creates both live sessions before targeting |
| Shared STREAM and datagram child | Two SAM clients on the same router; one owns both children and the other connects/sends to the owner's Destination | Open; runner now records separate STREAM and DATAGRAM rows |
| Shared lifecycle | Observe successful STREAM child removal and owner close invalidating the remaining DATAGRAM child | Open; implemented in runner, awaiting live artifact |
| Shared dialect disposition on the selected router | Try PRIMARY and MASTER; a supported dialect must pass the shared payload checks, and the other may be `unsupported` only on explicit rejection | Java has not yet been rerun through the artifact lane; old i2pd observations are informational and do not require a second router |
| Artifact and closure reconciliation | Validate artifacts, preserve commands and router/SAM versions, and reconcile M005/M006 additively | Open |

The known-service option makes loopback HTTP STREAM evidence reproducible in the main
conformance artifact. The other payload lanes use a second SAM client connected to the same
router bridge; they need no extra service tunnel. Full foundation closure still requires
live payload rows and a router with working inbound tunnels, so implementation of the
corrected lane does not by itself close M011.

## Invariants

- byte-exchange `pass` rows require exact payload agreement; the HTTP service operation
  requires a nonempty 2xx response;
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
python3 scripts/interop/qualify.py --router "Java I2P" --plan stream \
  --service-destination <hostname-or-b32.i2p>
python3 scripts/interop/qualify.py --router "Java I2P" --plan full \
  --peer-endpoint <same-router-peer-endpoint>
python3 scripts/check-conformance-artifact.py artifacts/interop/<router>-conformance.json
```

Run these commands locally or on an operator-provisioned host that can reach a SAM bridge
and the configured service. A same-router probe sets `--peer-endpoint` equal to
`--endpoint`; this tests two SAM clients through one router and does not claim cross-router
compatibility. The focused HTTP probe directly connects to a known service Destination;
the remaining payload rows create their own transient sender and receiver sessions.
M012 removed
the GitHub live workflow because no self-hosted Actions runner is registered and
GitHub-hosted loopback cannot reach operator routers.

## Acceptance criteria

1. Java I2P resolves a concrete I2P service Destination through SAM, sends an HTTP request
   over STREAM, and receives a valid 2xx HTTP response with a nonempty body through the
   harness. The router, SAM version, target, status, byte counts, and command are recorded.
2. D1, D2, D3, and RAW each produce a live payload row with correct trust typing; D1 and
   RAW cover both UDP-forwarded and control-socket modes, with explicit router rejection
   recorded as `unsupported` only when the router says so.
3. A supported MASTER or PRIMARY shared session passes separate STREAM and datagram child
   payload rows using one concrete Destination; the STREAM child is removed and owner close
   invalidates the remaining DATAGRAM child.
4. The feature matrix is exercised through one deployed router and SAM bridge. A second
   router implementation or cross-router tunnel is not required by this amended basis.
5. Every conformance artifact validates against the schema, and the matrix has no `fail`
   or unexplained `not_run` row.
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
endpoint is passed through to `sam-conformance`; it may be the same bridge endpoint.
UDP preflight is advisory; a known bridge is attempted regardless of arbitrary UDP probe
silence. The focused `sam-http-get` path and known-service runner lane resolve a service
Destination through SAM and test an HTTP request/response over STREAM. It can use a router's
own HTTP server tunnel and does not require cross-router tunnels. The full matrix still
requires live payload evidence for each in-scope feature, but it no longer requires an
external datagram receiver or a second router implementation: datagram and shared rows
create their receiver/peer SAM sessions through the selected router.

## 2026-10-08 — i2pd timeout and identity follow-up

The first pinned-i2pd retry used a 20-second SAM command timeout, even though its outer
runner timeout was 120 seconds. The official SAM v3 reference says session creation waits
for tunnel construction and may take a minute or more. The qualification harness and client
now default to a 120-second per-command timeout, independently configurable from the
process timeout. The retry also exposed an identity parsing defect: i2pd's I2P Base64 uses
`-` and `~`, which the previous standard-only Base64 parser rejected. The hash parser now
normalizes the alphabet, and a unit test covers that conversion.

With both fixes, pinned i2pd created both same-router sessions and the artifact recorded
their concrete identities, but the STREAM payload exchange still timed out. The validated
row remains `not_run`, so M011 is still open. Evidence and the bounded router-readiness
diagnosis are in `specs/live-router-qualification.md` and
`artifacts/interop/m011-2026-10-08/i2pd-long-timeout-stream/`. No cross-router tunnel is
needed for this test topology; a usable inbound tunnel and successful payload exchange are
still needed for live closure.
