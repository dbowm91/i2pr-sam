# Milestone 011 status — live-router payload qualification

Status: **closed**.

Implementation revision: `4c15aa02a30383a79cd2601ad589e73fdf62cb09` plus the
uncommitted working tree described below (client fixes proven live after that
commit; commit the tree before treating this record as final).
Closure revision: this record, committed immediately after the implementation
revision.

Plan: `plans/implementation/sam-library/011-live-router-payload-qualification.md`.

## Work completed

Live payload defects found and fixed against real routers:

- Java I2P folds the STREAM ACCEPT peer block onto one line
  (`$destination FROM_PORT=0 TO_PORT=0`); the peer reader accepts the
  single-line shape as well as the multi-line block (mock test E).
- Live payload is binary: a first payload line that is not valid UTF-8 is pushed
  back for the application, never mistaken for a blank peer-block terminator
  (mock test F). Before this fix the reader swallowed those bytes and the
  stream stalled forever after a parsed peer block.
- v1 `DATAGRAM SEND` / `RAW SEND` carries no reply on either router; the send
  path is fire-and-forget and delivery is proven by receipt (mock tests E/F
  reorder wire assertions after `recv()`).
- UDP-forward sessions retain their control socket for their whole lifetime;
  `close()` releases it (mock test L).
- Per-dialect session IDs, settle/attempt/tunnel-length environment controls,
  ACCEPT-before-CONNECT retry on `CANT_REACH_PEER`, and D3 I2P-alphabet
  normalization, as recorded in `specs/live-router-qualification.md`.

## Verification

| Command or check | Result |
|---|---|
| `cargo test --locked --workspace --all-targets` | pass — all suites green (incl. stream mock A–F, datagram mock E–L) |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | pass — 15 tests |
| `git diff --check` | pass |
| Java I2P full matrix + known-service HTTP lane (see command below) | pass — 11/11, fail=0, not_run=0 |

Qualification command (same-router peer, zero-hop tunnels):

```bash
SAM_CONFORMANCE_TUNNEL_LENGTH=0 SAM_CONFORMANCE_SETTLE_SECS=30 \
SAM_CONFORMANCE_ATTEMPTS=2 target/release/sam-conformance \
--endpoint 127.0.0.1:7656 --router "Java I2P" \
--router-version "2.13.1-1~ubuntu4" --plan full \
--service-destination hqlkjlb3ehnwjd2i64ikghjlmdrz3ws43yc2nehhdilvgwwpxjuq.b32.i2p \
--peer-endpoint 127.0.0.1:7656 --control-timeout 120 \
--datagram-endpoint 127.0.0.1:7655 \
--output artifacts/interop/m011-2026-10-08/java-i2p-conformance.json
```

Evidence: `artifacts/interop/m011-2026-10-08/java-i2p-conformance.json`
(schema 1.1, SAM 3.3, all ten capabilities `supported`) and the regenerated
`java-i2p-matrix.csv`. Row coverage: known-service HTTP STREAM (`200 OK`,
1,186 response bytes / 998 body bytes), same-router STREAM bidirectional
(512 bytes each way), D1/RAW over UDP-forward and control-socket, D2, D3 with
unverified-source typing, and PRIMARY + MASTER shared STREAM/DATAGRAM children
(one owner Destination each, child removal, owner-teardown invalidation).

## i2pd leg (not a closure prerequisite)

The amended M011 basis requires one router, not two. The i2pd leg was attempted
but is router-blocked, not client-blocked: the pinned `2.61.0-739-gd147bb0f`
build was lost when the host rebooted and wiped `/tmp`, and the system i2pd
2.49.0 repeatedly dies with a `Tunnels`-thread general-protection fault
(`dmesg: traps: Tunnels[...] general protection fault ... in i2pd`) within
minutes under session load — `SESSION CREATE` itself succeeds, then the process
vanishes mid-exchange (`Connection refused`). Historical i2pd artifacts in
`artifacts/interop/m011-2026-10-08/` are superseded observations, not evidence
against the client.

M011 is closed. Its live matrix satisfies the M005/M006 residual condition and
the M007/M008 dependency on M011 strict live closure.
