# Milestone 012 status — Live qualification harness correctness corrective

Status: **closed**.

Implementation revision: `97d5a9d0b9e59e6e5d58a5add1b169c4f20327ea` (`docs: remove unavailable live Actions workflow`).
Closure revision: this record, committed immediately after the implementation revision above.
Hosted CI evidence: workflow run [37718765511](https://github.com/dbowm91/i2pr-sam/actions/runs/37718765511), conclusion **success**, dispatched against implementation revision `97d5a9d0b9e59e6e5d58a5add1b169c4f20327ea`. All six jobs passed: fmt, clippy, test-linux, MSRV 1.89, macOS, and Windows. This record is documentation-only and adds no implementation changes.

Plan: `plans/implementation/sam-library/012-live-qualification-harness-correctness-corrective.md`.
Predecessor: `plans/implementation/sam-library/011-live-router-payload-qualification.md`.

## 1. Work completed

- `runner_command()` now builds a mutable argv and appends one `--peer-endpoint` pair only when configured. The endpoint is passed from `qualify_one()` through `run_runner()` to that argv.
- UDP probe silence from arbitrary public endpoints is represented as `unknown`. A reply proves only that one request/reply path works. UDP probing is advisory and cannot block an attempted SAM bridge.
- UDP result normalization recognizes `reachable`, `unreachable` only when positive evidence is supplied, and `unknown`. The deployed probe emits only `reachable` or `unknown`; it has no cooperative service and makes no hard negative claim.
- The hosted live workflow was removed. GitHub reported `total_count: 0` registered self-hosted runners on 2026-10-08, and GitHub-hosted loopback cannot reach operator routers. Live qualification instructions now give the local/operator-provisioned command and require `--peer-endpoint` for payload exchanges.
- The tracked Python bytecode file was removed; `.gitignore` excludes `__pycache__/` and `*.py[cod]`; Linux CI checks both the Python harness suite and tracked-file hygiene.
- M011, repository, harness, and environment documentation now treat actual router/bridge provisioning as the remaining requirement. Historical M005/M006 records were not rewritten; the live qualification environment record adds an explicit correction to its old UDP inference.

## 2. Harness regression evidence

`python3 -m unittest discover -s tests -p 'test_*.py'`: **11 tests passed**.

Coverage includes optional/single peer argv construction; the complete `qualify_one()` to
`run_runner()` peer handoff; live workflow removal and local command documentation; positive,
unknown, and evidence-gated UDP tri-state classification; advisory UDP handling; successful
and unavailable bridge probes; missing binary; `not_run` artifact synthesis; exit-code
precedence with invalid artifacts outranking blocked lanes; and Python cache ignore/index
hygiene.

## 3. Verification

Commands run on implementation revision `97d5a9d0b9e59e6e5d58a5add1b169c4f20327ea`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo test --locked --workspace --all-targets` | pass — 61 tests across 13 suites |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` | pass |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | pass — 11 tests |
| `python3 scripts/check-api-snapshot.py` | pass — 347 rows match |
| `python3 scripts/check-api-snapshot.py --self-test` | pass — 9/9 |
| `python3 scripts/check-proto-boundary.py` | pass |
| `python3 scripts/check-proto-boundary.py --self-test` | pass — 29/29 |
| `python3 scripts/check-conformance-artifact.py --self-test` | pass — all cases |
| tracked Python cache index check | pass — no tracked cache/bytecode |
| `git diff --check` | pass |

Hosted CI run `37718765511` completed successfully against `97d5a9d0b9e59e6e5d58a5add1b169c4f20327ea`. Its Linux job ran the
new harness and Python cache hygiene checks, and all stable, MSRV, macOS, Windows, Clippy,
and formatting jobs passed.

## 4. Workflow and M011 readiness decision

The repository has no registered self-hosted Actions runner. The live GitHub Actions
workflow was therefore removed rather than presented as a path to routers it cannot reach.
The harness remains executable locally or on an operator-provisioned host.

After M012, M011 no longer has repository readiness defects, but suitable provisioning is
still absent. A qualification attempt on 2026-10-08 ran:

```text
python3 scripts/interop/qualify.py --all --plan stream --skip-udp-probe \
  --artifact-dir /tmp/i2pr-sam-interop-check \
  --matrix-out /tmp/i2pr-sam-interop-check/matrix.csv
```

It exited `3` and synthesized valid `not_run` artifacts: Java I2P had no listener on
`127.0.0.1:7656`; `i2pd` and `i2pr` binaries were not on `PATH`. No live row ran and no
compatibility result is claimed. M011 stays blocked on provisioned, mutually reachable
routers and SAM bridges.

## 5. Successor decision

M011 is unblocked from the harness side. It remains **blocked on suitable two-router
provisioning**. M007 and M008 remain blocked on strict foundation closure, which still
requires M011's live payload evidence and M013's parity correction.
