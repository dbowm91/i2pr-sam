# Router-enabled interoperability harness

Repeatable, router-driven qualification for the I2P SAM v3 client. This harness
owns no protocol logic: it resolves a router pin, proves the live prerequisites
exist, invokes the repository conformance runner, validates the artifact the
runner produced, and merges everything into one comparable matrix.

Two invariants govern the design:

- **The harness never upgrades a result.** A row is `pass` only when the runner
  emitted `pass`. A lane that cannot start produces `not_run` rows for every
  feature in the plan, each carrying the blocking diagnostic category.
- **Every failure path names a diagnostic category.** No bare exception is
  swallowed and no traceback is discarded without a category attached.

`not_run` therefore means "the live lane did not run here", never "this router is
incompatible`.

One consequence worth stating plainly: the harness deletes any pre-existing
`--output` file before invoking the runner. Otherwise a stale artifact from an
earlier run would be read back as if the runner had just produced it, which is
how a failed build could masquerade as a completed lane.

## Files

| File | Role |
|---|---|
| `routers.json` | Declarative pins and provisioning hints for the three router targets. |
| `qualify.py` | Qualification driver: probes, runner invocation, validation, matrix, summary. |
| `udp_egress_probe.py` | Standalone advisory UDP reachability probe. Imports nothing from the harness. |
| `__init__.py` | Package marker and scope statement. |

## Exact commands

```bash
# Compile check
python3 -m py_compile scripts/interop/*.py

# Is there any observed UDP request/reply path? No reply means unknown, not blocked.
python3 scripts/interop/udp_egress_probe.py          # exit 0 = reply observed, 1 = unknown

# Qualify one router
python3 scripts/interop/qualify.py --router "Java I2P"
python3 scripts/interop/qualify.py --router i2pd --endpoint 127.0.0.1:7656

# Qualify every pinned router
python3 scripts/interop/qualify.py --all

# Machine-readable run record alongside the artifacts
python3 scripts/interop/qualify.py --all --json-summary artifacts/interop/run-summary.json
```

`qualify.py` flags: `--router LABEL` (repeatable), `--all`, `--endpoint HOST:PORT`,
`--peer-endpoint HOST:PORT` (second bridge used for peer-observed payload exchange)
(overrides `routers.json`), `--plan full|stream|datagram|shared`, `--artifact-dir`
(default `artifacts/interop`), `--matrix-out` (default
`artifacts/interop/interop-matrix.csv`), `--skip-udp-probe`, `--json-summary PATH`,
`--timeout` (per-router runner timeout, default 300 s).

Exit codes:

| Code | Meaning |
|---|---|
| 0 | Every requested router produced a valid artifact with no `fail` rows. |
| 1 | A `fail` row was present, or an artifact failed validation. |
| 2 | Usage error (unknown router label, unknown plan, bad timeout). |
| 3 | A requested router could not start its live lane for a named provisioning reason. |

Precedence: a hard result (exit 1) outranks a blocked lane (exit 3), because a
blocked lane already states that nothing was learned about the wire.

## Outputs

Per router, under `--artifact-dir`:

- `<slug>-conformance.json` — the runner's artifact when one was produced,
  otherwise a synthesized all-`not_run` artifact naming the blocking diagnostic.
- `<slug>-runner.stdout.log` / `<slug>-runner.stderr.log` — captured runner output.

Merged: `interop-matrix.csv` with header

```
router,router_version_or_sha,negotiated_sam_version,feature,operation,shared_dialect,result,diagnostic_category,local_destination_hash,notes
```

## Artifact contract

The harness consumes the JSON emitted by the conformance runner:

```
cargo run --locked --release -p i2pr-sam --bin sam-conformance -- \
  --endpoint HOST:PORT --router LABEL --router-version REV \
  [--plan full|stream|datagram|shared] [--output PATH]
```

Runner exit codes: `0` requested lanes produced no `fail` rows; `1` at least one
`fail` row; `2` usage/connection error; `3` provisioning error detected before
probing. The runner prints its JSON artifact to stdout whenever an artifact
exists.

Artifact shape (`schema_version` `1.1`, single object):

```json
{
  "schema_version": "1.1",
  "router": "Java I2P",
  "router_version_or_sha": "<rev>",
  "endpoint": "127.0.0.1:7656",
  "bridge_endpoint": "127.0.0.1:7656",
  "udp_endpoint": "127.0.0.1:7655",
  "negotiated_sam_version": "3.3",
  "generated_at": "2026-10-07T15:00:00Z",
  "plan": "full",
  "rows": [
    {
      "feature": "stream",
      "operation": "stream_payload_bidirectional",
      "shared_dialect": null,
      "local_identity": {"destination": "<base64>", "destination_hash": "<sha256 hex>"},
      "peer_identity": {"destination": "<base64>", "destination_hash": "<sha256 hex>"},
      "result": "pass",
      "evidence": {"bytes_sent": 24, "bytes_received": 24, "exact_payload_match": true, "identity_proven": true},
      "diagnostic_category": null,
      "notes": "free text, never private key material"
    }
  ],
  "summary": {"pass": 1, "unsupported": 0, "fail": 0, "not_run": 3, "create_only": 0, "capability_passes": 1}
}
```

`result` is one of `pass`, `unsupported`, `fail`, `not_run`, `create_only`.
Optional keys may be `null` when unknown. `create_only` is never counted as a
capability pass.

### Artifact validation

When `scripts/check-conformance-artifact.py` exists, `qualify.py` shells out to
it for each artifact and records its exit status as `artifact_schema_invalid` when
non-zero. **That file does not exist yet in this repository**, so runs today use
the built-in fallback: a minimal structural check that the artifact is a JSON
object with `schema_version` `1.1`, a non-empty `rows` list where every row has a
string `feature` and a `result` from the allowed set, and a `summary` object.
That fallback verifies shape only — it is not a substitute for schema validation.

## Diagnostic categories

Probes run in this order; the first blocking result stops the lane so the run
reports the root cause rather than a downstream symptom.

| Category | Meaning |
|---|---|
| `provisioning_binary_missing` | The router's documented binary is not on `PATH`. For Java I2P (`binary_probe: bridge_only`) this is informational, because it ships no single-binary entry point. |
| `provisioning_udp_egress_blocked` | Legacy category retained for compatibility; the UDP probe is advisory and never emits a blocking verdict. |
| `provisioning_port_closed` | No TCP listener on the SAM bridge endpoint. |
| `provisioning_bridge_unreachable` | A listener exists but does not answer `HELLO ...` with `HELLO REPLY`. |
| `runner_binary_missing` | `cargo` is unavailable, so `sam-conformance` cannot be built. |
| `runner_cli_mismatch` | The runner rejected the harness's CLI flags with an unknown-argument message. Harness/runner skew, not a protocol result. |
| `runner_connection_error` | The runner exited 2 with a usage or connection error. |
| `runner_provisioning_error` | The runner exited 3, reporting its own provisioning error. |
| `runner_timeout` | The runner exceeded `--timeout`. |
| `runner_build_failed` | `cargo` could not build `sam-conformance`. A repository condition, not a router result. |
| `runner_error` | The runner exited with a code outside its documented `0/1/2/3` set. |
| `artifact_absent` | The runner exited without producing parseable JSON. |
| `artifact_schema_invalid` | Artifact validation failed. |

## Known environment limitation (2026-10-07)

This harness has never exercised a live router. The development environment
cannot run one:

- no `i2pd` and no `i2pr` binary on `PATH`, and no running I2P router;
- Java I2P is not started, and there is no listener on 7656;
- access to the Docker daemon socket is denied (`docker ps` fails on
  `/var/run/docker.sock`), so containerized router provisioning is unavailable;
- the UDP probe reports positive replies or `unknown`; arbitrary endpoint silence
  cannot distinguish filtering from a non-responsive service, so it never blocks a
  known SAM bridge from being attempted.

Every live lane here ends in `not_run` with a provisioning diagnostic. That is an
environment fact, not a router compatibility verdict. Repeat the commands in a
router-enabled environment before claiming interoperability. `routers.json` marks
each entry `"verified_here": false` for the same reason; the recorded
`pinned_revision` values are source pins from `git ls-remote`, not evidence of
execution.

`i2pr` is qualified only through its own SAM bridge on the configured endpoint.
The harness never claims the router ran from inside this repository.

The manual GitHub Actions workflow runs on a self-hosted runner labeled
`i2p-interop`. That runner must host the configured routers or be able to route to
both the primary and peer SAM endpoints. GitHub-hosted loopback is isolated from
the operator's machine and is not a live qualification topology.

## Adding a router

1. Append an entry to `routers.json` with `label`, `repo_url`,
   `pinned_revision`, `bridge_endpoint`, `udp_endpoint`, `binary`,
   `binary_probe` (`path` or `bridge_only`), a `provisioning` object with real
   `start_commands`, `env` hints, and `notes`.
2. Obtain the pin with `git ls-remote <repo_url> HEAD` and record the observed
   SHA. Do not invent or copy a revision.
3. Prove the entry end to end in a router-enabled environment: the harness must
   reach `HELLO REPLY`, and the matrix must show the router's own rows rather
   than a synthesized `not_run` set.
4. Flip `"verified_here"` to `true` only for routers that have actually run.

New features belong in `PLAN_FEATURES` in `qualify.py` alongside the conformance
runner's own feature names, so a blocked lane still accounts for every requested
feature.
