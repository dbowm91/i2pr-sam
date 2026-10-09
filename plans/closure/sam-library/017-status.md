# Milestone 017 status — foundation closure, evidence reconciliation, and merge readiness

Status: **closed**.

Implementation/reconciliation revision: `c64a77dc3bab20e8b589253049f10601bf0c5457`
(M017 reconciliation + `closing` registry/roadmap; parent `28ee079` carried the
README/M011/conformance/API-CI/rustfmt/clippy reconciliation).
Closure revision: this record, committed on `main` immediately after the merge
below.
Merge revision: `46d06ea20ed0d4653e2af5ad9b4c017789a28342` (merge of PR #2 into
`main`).

Plan: `plans/implementation/sam-library/017-foundation-closure-reconciliation-and-merge-readiness.md`.
Repository baseline at registration: `de6bd3258105e6694ded1fdc5ef81abccd2d02fe`
on `plans/012-013-foundation-qualification-correctives`.

## 1. Work completed (no new SAM capability)

- README now reports strict foundation closure: Java I2P 11/11 live pass (SAM 3.3,
  router `2.13.1-1~ubuntu4`), i2pd partial/incomplete (not a prerequisite), i2pr
  not live-qualified, single-router basis with no cross-router claim.
- M011 plan status corrected `active` → `closed` with a current-authority note;
  the “Full closure evidence still required” table is marked historical
  pre-closure context. i2pd attempt history preserved.
- M011 closure metadata corrected additively: final integrated
  implementation/evidence/closure revision is `de6bd32`; parent `4c15aa02` is the
  last pre-final-fix commit; the “uncommitted working tree” wording described work
  committed in `de6bd32`. No live result changed.
- `specs/conformance-observations.csv` regenerated as the current compact summary
  (Java 11 pass rows with matrix-matching Destination hashes; i2pd HELLO/SAM 3.3/
  identity observations with `unsupported` PRIMARY vs `not_run` payload rows kept
  distinct; i2pr `not_qualified` rows with no claim). The pre-M011 all-`not_run`
  table is preserved at `specs/conformance-observations.2026-10-07-not-run.csv`.
- `specs/live-router-qualification.md` carries an M017 current-authority note;
  historical attempt narrative preserved.
- API snapshot comparison promoted from informational (`continue-on-error`) to a
  required CI guard: `api/public-types.txt` (347 declarations) matches the head,
  so ordinary CI now fails on unregistered public API drift.
- `cargo fmt` conformance restored (rustfmt-version drift only, no semantics).
- Conformance-runner clippy cleanliness restored with no wire behavior change:
  `Outcome::CreateOnly` variant and `Row::created_only` retained with
  `#[allow(dead_code)]` for schema-vocabulary completeness, and the single-shot
  outer `loop` around the datagram retry `for` loop restructured to a plain
  `for` (fixes `clippy::never-loop` on the current toolchain).

## 2. Verification (local, exact-head `c64a77d` before merge)

| Command or check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo check --locked --workspace --all-targets` | pass — 0 errors |
| `cargo test --locked --workspace --all-targets` | pass — 68 tests across 14 suites |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass — no issues |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` | pass |
| `cargo +1.89 check --locked --workspace --all-targets` | pass |
| `cargo +1.89 test --locked --workspace --all-targets` | pass |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | pass — 15 tests |
| `python3 scripts/check-api-snapshot.py` | pass — 347 rows match |
| `python3 scripts/check-api-snapshot.py --self-test` | pass — 9/9 |
| `python3 scripts/check-proto-boundary.py` | pass |
| `python3 scripts/check-proto-boundary.py --self-test` | pass — 29/29 |
| `python3 scripts/check-conformance-artifact.py --self-test` | pass — all cases |
| `python3 scripts/check-conformance-artifact.py artifacts/interop/m011-2026-10-08/java-i2p-conformance.json` | valid |
| conformance-summary consistency (CSV vs matrix vs artifact) | Java 11/11 pass, hashes match, 0 missing |
| stale-status text check (README/M011) | pass — remaining “M011 remains open” hits are inside preserved historical attempt narrative only |
| tracked Python cache guard | pass — no tracked bytecode |
| `git diff --check` | pass |

Protocol/reference revisions used: pinned SAM v3 reference freeze per
`specs/references/sam-v3-reference-freeze.md` (unchanged by M017); live evidence
is the M011 Java I2P artifact (schema 1.1, SAM 3.3, 11 pass / 0 fail /
0 not_run), re-validated, not re-run — no new live campaign is in scope.

## 3. Final Java artifact and current qualification summary

- Validation: `conformance artifact valid:
  artifacts/interop/m011-2026-10-08/java-i2p-conformance.json`.
- API surface: 347 declarations, zero drift (`public API snapshot matches
  api/public-types.txt (347 rows)`).
- Java I2P (`2.13.1-1~ubuntu4`, SAM 3.3): 11/11 pass — known-service HTTP STREAM
  (`200 OK`, 1,186 response / 998 body bytes), D1/RAW × UDP-forward +
  control-socket, D2, D3 (unverified-source typing), PRIMARY + MASTER shared
  STREAM/DATAGRAM children (one owner Destination each, child removal,
  owner-teardown invalidation).
- i2pd (pinned `d147bb0f…`, SAM 3.3 HELLO ok, `NAMING LOOKUP NAME=ME` ok after
  I2P-Base64 normalization): explicit `Unknown STYLE` rejection of PRIMARY
  (`unsupported`, MASTER spelling retained); payload rows `not_run`
  (`peer_identity_unavailable` / `router_timeout`, no usable inbound tunnel;
  system 2.49.0 dies under session load). Router readiness limitation, not
  client failure, not a closure prerequisite.
- i2pr: not live-qualified by this repository (`not_qualified`); no
  interoperability claim.
- No cross-router/multi-router interoperability is claimed. “Foundation closed”
  is not rewritten as “multi-router interoperability proven.”

## 4. Merge vehicle and PR disposition

- Final PR: #2 (`Milestone 017: foundation closure reconciliation and merge
  readiness`), head `c64a77dc3bab20e8b589253049f10601bf0c5457`, base `main`,
  merged as `46d06ea20ed0d4653e2af5ad9b4c017789a28342` with a merge commit
  preserving all 26 foundation commits ahead of the old `main`.
- Exact-head hosted CI on the PR head: run `37948058059`, conclusion **success**
  (fmt, clippy, test-linux, msrv 1.89, test-macos, test-windows all pass; prior
  green run `37946641806` on `28ee079` covered the same tree before the
  `closing`-status registry commit).
- Stale PR #1 (`Milestones 005-006`, head `15f9308`, M005/M006-only): never
  merged as a standalone diff. After the PR #2 merge its head became reachable
  from `main`, so GitHub auto-resolved it as merged-by-ancestry at the PR #2
  merge time; the complete foundation reached `main` only through PR #2 merge
  `46d06ea`. A superseding comment was left on PR #1 before the merge. No
  incomplete-foundation merge occurred.
- Post-merge `main` verification: `c64a77d` proven ancestor of `main`
  (`git merge-base --is-ancestor`); `main` contains the final Java artifact,
  current registry, and reconciled docs; API snapshot re-checked green on
  `main`; post-merge `main` CI run `37950049744`, conclusion **success**.

## 5. Branch disposition (ancestry-proven, post-merge)

| Branch | Disposition |
|---|---|
| `plans/001-004-sam-foundation` | deleted (remote + local) — tip ancestor of `main` |
| `plans/005-006-foundation-correctives` | deleted (remote + local) — tip ancestor of `main` |
| `plans/012-013-foundation-qualification-correctives` | deleted (remote + local) — tip ancestor of `main` |
| `plans/017-foundation-closure-reconciliation` | retained until this closure lands on `main`, then deleted (remote + local) — tip ancestor of `main` via merge `46d06ea` |

No branch with unique commits was deleted.

## 6. Deviations, residual risk, and deferred work

- Deviation from plan §6G preference log: PR #1 was not manually closed without
  merge; GitHub auto-resolved it as merged-by-ancestry when PR #2 landed. Effect
  is equivalent (no standalone incomplete merge), recorded explicitly in §4.
- `cargo fmt` and `clippy::never-loop` findings were toolchain-drift hygiene, not
  protocol defects; fixed without wire/API behavior change, so no new corrective
  plan was registered (per §8, exact-head failures would have stopped the merge;
  these were fixed and re-verified green before merge).
- M011 closure prose mentions a same-router STREAM bidirectional 512-byte
  exchange; the final artifact's 11 rows are HTTP STREAM + D1/RAW/D2/D3/shared
  (no standalone bidirectional STREAM row). The 11/11 count is exact; the
  current conformance summary (§3) reports actual artifact operations.
- Residual risk: single-router Java qualification only; i2pd/i2pr/multi-router
  hardening is future maturity work. License selection still required before
  public package distribution. No crate publication occurred.

## 7. Successor unblock audit

- **M007 C ABI + Python bindings**: technical prerequisites satisfied (M011
  strict live closure + M013 parity, both closed; M017 merged to `main`).
  Research/planning/implementation may proceed against the merged pre-1.0 Rust
  foundation. License selection required before public package/wheel
  distribution, not before implementation. → **unblocked for implementation
  planning**.
- **M008 i2pr service-tunnel adapter**: still **blocked** on a stable merged
  `i2pr-service-tunnels` integration revision consumable from i2pr (i2pr Plan 379
  closed on its work branch with the current public consumer contract, but this
  repository must consume a stable selected revision at implementation time).
- **M009/M010**: remain transitively blocked on M008.
- Broader i2pd/i2pr/multi-router compatibility hardening: future maturity work,
  not a reason to reopen this foundation closure.

M017 is the last foundation-only milestone. No SAM protocol/runtime behavior was
changed by reconciliation.
