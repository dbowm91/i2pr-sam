# Live-router qualification attempt

## M012 correction to the historical UDP interpretation

The 2026-10-07 observation below remains a record of what that probe saw, but its
interpretation as proof that UDP egress above port 1024 was blocked was not justified:
the arbitrary UDP endpoints did not promise to answer the payloads sent. No response is
now represented as `unknown`, not as blocked. The harness treats this probe as advisory
and continues to the configured SAM bridge. A missing router or unreachable bridge remains
the current environment blocker; no live router payload row has passed. See M012 closure
`plans/closure/sam-library/012-status.md` for the code and regression evidence.

Attempted 2026-10-07 by milestone 005 with the repository harness
(`scripts/interop/qualify.py`) and the pinned revisions in
`references/sam-v3-reference-freeze.md`.

## Why no live lane could start

The UDP observation in this historical run records replies and non-replies at the named
endpoints only. The conclusion below that port 53 was the sole permitted UDP egress was an
unsupported inference and is superseded by M012. The only currently re-established blocker
is the absence of provisioned routers and reachable SAM bridges, as recorded in the M012
closure attempt.

Two independent environment prerequisites failed. Neither is a router protocol failure,
and neither may be recorded as one.

| Prerequisite | Probe | Observed |
|---|---|---|
| router binary | `i2pd` / `i2pr` on `PATH` | neither installed; Java I2P is not a single binary |
| container runtime | `docker ps` | `permission denied` on `/var/run/docker.sock` |
| bridge listener | TCP connect `127.0.0.1:7656` | refused; no process on 7656-7658 |
| **UDP egress for peer transport** | `python3 scripts/interop/udp_egress_probe.py` | answers on `1.1.1.1:53` only; `443/UDP` and non-standard `34567/UDP` are silently dropped |

The UDP result is the decisive blocker and is new information relative to milestone 004.
A host that answers DNS but drops every UDP packet above port 1024 cannot run an I2P
router at all: SSU peering uses random high UDP ports. Even a fully installed router would
reseed and then fail to build a single tunnel, so no payload row could have passed.

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

No live payload row is a pass. No negotiated SAM version was observed. No claim about
Java I2P, i2pd, or i2pr wire behaviour is made here; the harness exists precisely so that
such a claim can only be produced by an actual exchange. Milestone 011 carries the live
payload matrix as its objective.
