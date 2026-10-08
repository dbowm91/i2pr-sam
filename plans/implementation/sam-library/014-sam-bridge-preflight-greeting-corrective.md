# 014 — SAM bridge preflight greeting corrective

Class: corrective harness invariant.

Status: **closed**; see `plans/closure/sam-library/014-status.md`.

## Objective

Make live qualification's bridge preflight send the SAM v3 `HELLO` request accepted by
both reference Java I2P and i2pd, and prevent the incorrect spelling from returning.

## Finding

During M011 provisioning, the preflight sent `HELLO VERSION=3.3 MIN=3.0 MAX=3.3`. SAM v3
uses `HELLO VERSION MIN=3.0 MAX=3.3`. Both routers closed the malformed request, and the
harness incorrectly reported an otherwise listening bridge as unreachable. The conformance
client already sent the correct form; the mismatch was isolated to `probe_bridge()`.

## Scope

- correct the preflight request to match the SAM v3 grammar used by the client;
- make the bridge-probe regression test assert the exact bytes sent;
- run the Python harness suite and verify the corrected preflight against live Java I2P
  and i2pd bridges.

## Acceptance criteria

1. The request is exactly `HELLO VERSION MIN=3.0 MAX=3.3\n`.
2. The harness regression suite checks the exact request bytes.
3. Both provisioned bridges answer `HELLO REPLY` and pass the preflight.

## Verification

```bash
python3 -m unittest discover -s tests -p 'test_interop_qualify.py'
python3 scripts/interop/qualify.py --router i2pd --endpoint 127.0.0.1:7660 --skip-udp-probe
python3 scripts/interop/qualify.py --router 'Java I2P' --endpoint 127.0.0.1:7656 --skip-udp-probe
```

## Closure evidence

Record the implementation and closure revisions, harness test result, and live bridge
greetings in `plans/closure/sam-library/014-status.md`. This corrective enables M011; it
does not replace M011's router-to-router payload matrix.
