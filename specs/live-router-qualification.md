# Live-router qualification attempt

## M012 correction to the historical UDP interpretation

The 2026-10-07 observation below remains a record of what that probe saw, but its
interpretation as proof that UDP egress above port 1024 was blocked was not justified:
the arbitrary UDP endpoints did not promise to answer the payloads sent. No response is
now represented as `unknown`, not as blocked. The harness treats this probe as advisory
and continues to the configured SAM bridge. No live router payload row has passed. See M012
closure `plans/closure/sam-library/012-status.md` for the code and regression evidence; the
latest provisioning and tunnel-connectivity attempt is recorded below.

Attempted 2026-10-07 by milestone 005 with the repository harness
(`scripts/interop/qualify.py`) and the pinned revisions in
`references/sam-v3-reference-freeze.md`.

## Why no live lane could start

The UDP observation in this historical run records replies and non-replies at the named
endpoints only. The conclusion below that port 53 was the sole permitted UDP egress was an
unsupported inference and is superseded by M012. The table below records the 2026-10-07
baseline; the current blocker is described in the 2026-10-08 attempt below.

Two independent environment prerequisites failed. Neither is a router protocol failure,
and neither may be recorded as one.

| Prerequisite | Probe | Observed |
|---|---|---|
| router binary | `i2pd` / `i2pr` on `PATH` | neither installed; Java I2P is not a single binary |
| container runtime | `docker ps` | `permission denied` on `/var/run/docker.sock` |
| bridge listener | TCP connect `127.0.0.1:7656` | refused; no process on 7656-7658 |
| **UDP egress for peer transport** | `python3 scripts/interop/udp_egress_probe.py` | answers on `1.1.1.1:53` only; `443/UDP` and non-standard `34567/UDP` are silently dropped |

The original UDP interpretation was not supported by the probe and is superseded by M012.

## Commands and results

| Router | Invocation | Result |
|---|---|---|
| Java I2P | `python3 scripts/interop/qualify.py --router 'Java I2P' --endpoint 127.0.0.1:7656` | exit 3, `provisioning_udp_egress_blocked` (informational `provisioning_binary_missing`) |
| i2pd | `python3 scripts/interop/qualify.py --router i2pd --endpoint 127.0.0.1:7656` | exit 3, `provisioning_binary_missing` |
| i2pr | `python3 scripts/interop/qualify.py --router i2pr --endpoint 127.0.0.1:7656` | exit 3, `provisioning_binary_missing` |

Artifacts written by those runs validate against `specs/conformance.schema.json` and
contain `not_run` rows with `pass=0` and `capability_passes=0`. Exit code 3 means "the lane
could not be provisioned", never "the router failed a row".

## What this record does not claim

No live payload row is a pass. No payload compatibility claim is made here; the harness
exists precisely so that such a claim can only be produced by an actual exchange.
Milestone 011 carries the live payload matrix as its objective.

## 2026-10-08 — provisioned routers and reachable SAM bridges

After explicit authorization to install routers, this host ran Java I2P and i2pd with
separate SAM listeners. The M014 greeting correction allowed the harness to recognize both
bridges. The pinned i2pd revision `d147bb0fd6789c75dc1c4d70c4f79b151a552d53` was built from
source as i2pd `2.61.0-739-gd147bb0f` (0.9.70); it used the host's existing i2pd router
database. Java I2P was installed as Ubuntu package `2.13.1-1~ubuntu4`; its router log
reported runtime `2.13.0-0-1~ubuntu4`.

| Router | SAM endpoint | Preflight / negotiated version | Full-plan result |
|---|---:|---|---|
| Java I2P | `127.0.0.1:7656` | HELLO REPLY; SAM 3.3 | pass=0, unsupported=0, fail=0, not_run=9 |
| i2pd | `127.0.0.1:19856` | HELLO REPLY; SAM 3.3 | pass=0, unsupported=1, fail=0, not_run=8 |

The single i2pd unsupported row is PRIMARY shared style; i2pd replied with its explicit
`I2P_ERROR MESSAGE="Unknown STYLE"` verdict and retains the MASTER spelling. The other
i2pd shared row was `not_run`. Stream, DATAGRAM, RAW, DATAGRAM2, and DATAGRAM3 did not
exchange payloads: the runner could not resolve concrete local/peer Destinations. i2pd's
router log reported `Can't create inbound tunnel, no peers available`. The UDP probe
observed `1.1.1.1:53` and returned `null` for high-port reachability, so this attempt does
not establish that high UDP is blocked.

The validated artifacts, per-router matrices, combined matrix, and runner logs are in
[`artifacts/interop/m011-2026-10-08/`](../artifacts/interop/m011-2026-10-08/). The exact
qualification code revision was `75d1a13f7a2f09aea720c7489ec8d7ac4ab67317`. The full
matrix did not resolve its transient peer identity, so its `not_run` rows do not indicate
router incompatibility.

## 2026-10-08 — SAM request to the Java router's own I2P webserver

Following the same-router route, Java I2P's existing `I2P webserver` tunnel was started and
the repository's `sam-http-get` example sent `GET /` over a SAM STREAM session to the
tunnel's `.b32.i2p` Destination. The service was hosted by the same router as the SAM
bridge. SAM name resolution succeeded and the HTTP response was `200 OK` with 1,186 bytes.
The exact command and output are recorded in
[`artifacts/interop/m011-2026-10-08/attempt.md`](../artifacts/interop/m011-2026-10-08/attempt.md).

This is live payload evidence through the router's own tunnels and does not require a
second router. The standalone full-matrix runner still cannot resolve the transient
identity it uses for its generated peer, so DATAGRAM/RAW/shared rows remain unqualified.
M011 now continues against one router and a valid service Destination; it is active for
the remaining feature evidence rather than blocked on cross-router connectivity.

A follow-up ran pinned i2pd with both harness endpoints set to `127.0.0.1:19856` and a
temporary local UDP echo server tunnel. The matrix still had zero payload passes: eight
rows were `not_run` because transient peer identity resolution failed, PRIMARY was explicitly
`unsupported`, and MASTER timed out. A direct datagram echo probe also timed out. The
schema-valid artifact and matrix are under
`artifacts/interop/m011-2026-10-08/i2pd-same-router-udpserver-*`. This confirms the remaining
runner limitation is its generated-peer setup; a known-destination lane and a functioning
receive service are needed to qualify DATAGRAM/RAW. Shared-session validation still needs
an in-router SAM peer that can observe child identity. These outcomes do not indicate a
cross-router incompatibility.
