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
`artifacts/interop/m011-2026-10-08/i2pd-same-router-udpserver-*`. The runner limitation at
that revision was its generated-peer setup. The correction and current same-router
requirements are recorded below; these historical outcomes do not indicate a cross-router
incompatibility.

## Known-service STREAM lane and remaining closure prerequisites

`sam-conformance` and `qualify.py` now accept `--service-destination HOSTNAME|B32`. The
runner resolves it using SAM, opens a transient STREAM session, sends `GET /`, and emits a
`stream_http_service_request_response` row. A pass requires an HTTP 2xx status and a
nonempty response body; the artifact records the service Destination/hash and structured
HTTP status/body byte evidence. This path uses one router and does not require a second
router or a cross-router tunnel. Re-run against Java I2P's active webserver tunnel to
replace the earlier standalone-example evidence with a runner artifact.

The remaining feature closure requires more than a known hostname:

- D1/RAW UDP-forwarded and control-socket rows create actual transient receiver sessions
  through the same router as the sender.
- D2/D3 create matching same-router receiver sessions and check authenticated D2 identity
  versus unverified D3 source behavior.
- Shared rows use a second SAM client on the same router to receive STREAM/datagram child
  payloads, verify the shared owner's concrete Destination, remove a child, and observe
  owner teardown. No service tunnel or second router is required for these exchanges.
- On the selected router, test PRIMARY and MASTER and use whichever supported dialect
  passes the shared payload checks. An explicit rejection is `unsupported`; timeout,
  missing identity, or tunnel unavailability is `not_run`. Existing i2pd observations are
  useful history but do not make a second router implementation a closure prerequisite.

Negotiated SAM 3.3 alone does not establish that each 3.3 capability works. The current
official SAM reference describes the shared owner/subsession mechanism, notes that i2pd
still calls the owner `MASTER`, and says i2pd does not support most SAM 3.3 features. It
also distinguishes authenticated/repliable D1 and D2 from unauthenticated D3 and identity-
free RAW. Therefore qualify each feature independently and treat an explicit style rejection
as router-specific `unsupported`, not as evidence about unrelated rows. See the
[SAM v3 reference](https://i2p.net/en/docs/api/samv3/).

The [M011 plan](../plans/implementation/sam-library/011-live-router-payload-qualification.md)
tracks these rows and the additive M005/M006 closure reconciliation.

## 2026-10-08 — same-router harness correction

Review of the runner found that it resolved a peer Destination in a temporary session,
closed that session, then sent to that identity from a different transient session. The
ordinary STREAM path also connected to the sending session's identity instead of the
accepting session's identity. Datagram rows had the equivalent sender/receiver reversal.
`sam-conformance` now creates the actual sessions first, targets the receiving session's
concrete Destination, and checks the observed source against the actual sender session.
Shared dialect coverage now emits separate STREAM-child and DATAGRAM-child rows, proves
that both children expose the owner's Destination, removes the STREAM child, and checks that
closing the owner invalidates the remaining DATAGRAM child. Blocked shared-plan synthesis
now accounts for both operations per dialect.

This correction needs only two SAM clients connected through one router. The DATAGRAM/RAW
rows use the actual transient receiver session and local forwarding socket; they do not
depend on a separately configured I2P UDP echo tunnel. The router still needs working
tunnels for the exchanges to complete, and this harness correction is not itself live
router evidence.

The corrected binary passed `cargo check --locked -p i2pr-sam --bin sam-conformance`; the
qualification harness tests passed (13 tests), and the artifact validator self-test passed.
A qualification preflight against `127.0.0.1:17656` exited `3` with
`provisioning_port_closed` and synthesized four valid `not_run` rows for the two shared
operations under PRIMARY and MASTER. No SAM bridge was listening on this host during the
attempt, so no live payload row ran and M011 remains open. The official
[SAM v3 reference](https://i2p.net/en/docs/api/samv3/) documents shared subsessions as
using one Destination/tunnel set, and notes that i2pd calls the dialect MASTER; these facts
support the single-router test topology but do not replace runtime evidence.

## 2026-10-08 — router startup retry

The system i2pd 2.49.0 package started from a fresh home directory and opened SAM, but
crashed with `malloc(): corrupted top size` while a transient SAM STREAM Destination was
being created. I retried using the already-built pinned i2pd `2.61.0-739-gd147bb0f`
(`d147bb0fd6789c75dc1c4d70c4f79b151a552d53`) in an isolated home data directory. It remains
running with SAM on `127.0.0.1:17656` and negotiated SAM 3.3.

The same-router qualification command was:

```bash
python3 scripts/interop/qualify.py --router i2pd \
  --endpoint 127.0.0.1:17656 --peer-endpoint 127.0.0.1:17656 \
  --plan stream --skip-udp-probe --timeout 120 \
  --artifact-dir /tmp/m011-retry-pinned-stream \
  --matrix-out /tmp/m011-retry-pinned-stream/matrix.csv
```

The bridge and artifact schema checks passed, but the STREAM row was `not_run` with
`router_timeout` during connecting-session creation, before payload exchange. The validated
artifact and matrix are in
[`artifacts/interop/m011-2026-10-08/i2pd-retry-pinned-stream/`](../artifacts/interop/m011-2026-10-08/i2pd-retry-pinned-stream/).
This retry shows the router process was not being held off by another agent; it does not
provide a live payload pass or close M011.

## 2026-10-08 — same-router timeout and identity diagnosis

The first run's 20-second per-command timeout was shorter than router tunnel construction.
The official [SAM v3 reference](https://i2p.net/en/docs/api/samv3/) says `SESSION CREATE`
can take a minute or more while tunnels are built, so it advises against a short timeout.
The runner's 120-second process timeout did not change the conformance binary's separate
20-second SAM-command timeout. The harness now exposes `--control-timeout`, defaults it to
120 seconds throughout, and records the distinction from the overall runner timeout.

With a longer command timeout, both i2pd sessions were created, but the runner reported
`identity_unavailable`. A direct SAM `NAMING LOOKUP NAME=ME` returned `RESULT=OK` and a
valid Destination. The actual parsing defect was that i2pd uses I2P Base64 (`-` and `~`)
where the Rust parser accepted only the standard Base64 alphabet (`+` and `/`). The
Destination hash parser now normalizes those two alphabet characters before decoding.
Pinned [i2pd SAM code](https://github.com/PurpleI2P/i2pd/blob/d147bb0fd6789c75dc1c4d70c4f79b151a552d53/libi2pd_client/SAM.cpp)
implements `NAME=ME` and delays successful session status until the local Destination is
ready.

After these fixes, a run with `--control-timeout 120` recorded both local and peer
Destination identities, then timed out waiting for the STREAM payload exchange. The
schema-valid result is preserved in
[`artifacts/interop/m011-2026-10-08/i2pd-long-timeout-stream/`](../artifacts/interop/m011-2026-10-08/i2pd-long-timeout-stream/).
The row remains `not_run`; identity resolution and successful `SESSION CREATE` are not
payload evidence. The pinned [i2pd `Destination::IsReady()` implementation](https://github.com/PurpleI2P/i2pd/blob/d147bb0fd6789c75dc1c4d70c4f79b151a552d53/libi2pd/Destination.h)
requires a LeaseSet and outbound tunnels, but does not establish that an inbound tunnel is
available. The observed peer transport resets/EOFs and the earlier explicit “no peers
available” inbound-tunnel log are consistent with this isolated router lacking usable
inbound tunnels. This is a router connectivity/readiness limitation after SAM setup, not
evidence that a second router or cross-router tunnels are needed, nor evidence of a SAM
dialect incompatibility.

The artifact validator accepted the new result. M011 remains open: a payload exchange
through the same router, or the known-service HTTP STREAM operation, must pass before its
live evidence gate can close.

## 2026-10-08 — full matrix pass on Java I2P; M011 closed

The blocking defect was in the client's peer-block reader, found by comparing a raw
Python probe against the runner: after Java's single-line peer block
(`$destination FROM_PORT=0 TO_PORT=0`, ~546 bytes, no blank line, no EOF), the first
binary payload line (starting `0xED 0x0C…`, not valid UTF-8) was decoded with
`unwrap_or_default()`, read as an empty terminator, and its bytes were swallowed —
so `read_exact(512)` stalled forever waiting for bytes already consumed. Mock payloads
were ASCII and never covered this. The loop now works on bytes
(`strip_line_ending`), pushes back non-UTF-8 lines, and accepts inline
`FROM_PORT=`/`TO_PORT=` on the destination line. v1 sends were also made
fire-and-forget (neither router replies to `DATAGRAM SEND`/`RAW SEND`), and
UDP-forward sessions retain their control socket until `close()`.

Final qualification (same-router peer, zero-hop tunnels, settle 30 s, 2 attempts,
control timeout 120 s) against Java I2P package `2.13.1-1~ubuntu4` (runtime
`2.13.0-0-1~ubuntu4`), SAM TCP `127.0.0.1:7656`, negotiated SAM 3.3:

- `stream_http_service_request_response` **pass** — HTTP `200 OK`, 1,186 response
  bytes through the router's own eepsite tunnel.
- `stream_payload_bidirectional` **pass** — 512 bytes each way, same-router peer.
- D1/RAW over UDP-forward and control-socket, D2, D3: **pass** with correct trust
  typing (D3 source unverified hash, RAW sourceless).
- PRIMARY and MASTER shared STREAM/DATAGRAM children: **pass** — one owner
  Destination each, child removal, owner-teardown invalidation.

Summary: `pass=11, fail=0, not_run=0, unsupported=0`; all ten capabilities
`supported`. Evidence:
[`artifacts/interop/m011-2026-10-08/java-i2p-conformance.json`](../artifacts/interop/m011-2026-10-08/java-i2p-conformance.json)
and the regenerated `java-i2p-matrix.csv` in the same directory. Older files in
that directory (`interop-matrix.csv`, the first `java-i2p-*` run, the i2pd
attempts) are superseded history.

The i2pd leg was not completed and is not required by the amended single-router
basis: the pinned build was lost in a host reboot that wiped `/tmp`, and system
i2pd 2.49.0 dies within minutes under session load (`Tunnels`-thread general
protection fault; `SESSION CREATE` succeeds, then the process vanishes
mid-exchange). See `plans/closure/sam-library/011-status.md`. M011 is closed;
its matrix satisfies the M005/M006 residual live-payload condition.
