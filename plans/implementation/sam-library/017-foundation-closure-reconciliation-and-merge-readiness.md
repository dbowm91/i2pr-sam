# 017 — Foundation closure, evidence reconciliation, and merge-readiness pass

Class: closure + reconciliation + repository hygiene.

Status: **ready**.

Repository baseline: `de6bd3258105e6694ded1fdc5ef81abccd2d02fe` on
`plans/012-013-foundation-qualification-correctives`.

Corrects current-authority drift after M011–M016 and prepares the complete foundation for
integration into `main`.

## 1. Objective

Finish the SAM foundation as a repository state, not merely as an implementation branch.

The Rust implementation and Java I2P live qualification are materially complete, but the
repository currently has four conflicting authorities:

1. the registry/roadmap say strict foundation closure;
2. the README and `specs/conformance-observations.csv` still describe the pre-M011
   all-`not_run` state;
3. the M011 plan still starts as `active` and contains now-satisfied “closure evidence
   still required” prose;
4. the M011 closure record says its implementation is
   `4c15aa02... plus the uncommitted working tree`, even though the final code, artifact,
   and closure were committed together in `de6bd3258105e6694ded1fdc5ef81abccd2d02fe`.

Repository integration is also incomplete:

- `main` is still the initialization commit `af3cce5ac76f...`;
- the complete implementation branch is 23 commits ahead of `main` at this baseline;
- PR #1 still points at `plans/005-006-foundation-correctives` / `15f930890406...` and
  therefore omits M011–M016;
- hosted CI has not run at the final post-live implementation head.

M017 reconciles those authorities, obtains exact-head CI, establishes the correct merge
vehicle, merges the full foundation to `main`, and retires superseded planning branches.

No new SAM capability is in scope.

## 2. Why this milestone is ready

Every implementation milestone currently registered through M016 is closed.

The final Java I2P artifact at:

`artifacts/interop/m011-2026-10-08/java-i2p-conformance.json`

records:

- schema 1.1;
- SAM 3.3;
- 11 passes;
- zero failures;
- zero `not_run` rows;
- known-service HTTP STREAM;
- same-router bidirectional STREAM;
- DATAGRAM1 and RAW over UDP forwarding and control socket;
- DATAGRAM2;
- DATAGRAM3 with unverified-source semantics;
- PRIMARY and MASTER shared STREAM + DATAGRAM children with one concrete Destination per
  owner and lifecycle evidence.

The final code and closure artifact were committed together at
`de6bd3258105e6694ded1fdc5ef81abccd2d02fe`. That commit contains the post-live client fixes, regression tests,
qualification artifact, registry/roadmap updates, and `011-status.md`.

M012/M013 closed the harness and blocking-parity gaps. M014–M016 closed live-discovered
preflight, runner-dispatch, and i2pd verdict defects.

The remaining work is therefore integration/evidence truth, not protocol implementation.

## 3. Current evidence and known reconciliation defects

### Current implementation authority

Branch:

`plans/012-013-foundation-qualification-correctives`

Head:

`de6bd3258105e6694ded1fdc5ef81abccd2d02fe`

Current diff against `main` at registration time:

- 23 commits ahead;
- zero behind;
- 116 changed files.

### Hosted CI

The last hosted CI success is earlier than the final M011 work. The final head currently
has no combined GitHub status checks.

M017 must therefore obtain hosted CI against an exact post-reconciliation head before
merge and normal `main` CI after merge.

### Stale README

The README still says:

- foundation is “conditionally closed”;
- the live matrix has no passing rows;
- M011 remains open.

All three are stale.

### Stale conformance summary

`specs/conformance-observations.csv` is still the original blocked/not-run observation
table and includes the superseded assertion
`provisioning_udp_egress_blocked`.

That historical evidence should remain available, but it is not a truthful current
conformance summary.

### M011 plan/closure metadata

The M011 plan's opening status is stale.

The M011 closure's implementation metadata is also stale/self-referential:

```text
Implementation revision: 4c15aa... plus the uncommitted working tree...
```

The final client fixes and closure record were committed together at `de6bd3258105e6694ded1fdc5ef81abccd2d02fe`.
M017 must record that fact additively and remove any current-authority ambiguity without
rewriting the substantive live evidence.

### PR state

Open PR #1:

- title: `Milestones 005-006: protocol correctness corrective + verification/CI`;
- head: `plans/005-006-foundation-correctives`;
- head SHA: `15f930890406...`;
- base: `main`.

It is not the merge vehicle for the complete foundation.

## 4. Invariants

- No SAM wire behavior changes unless an exact-head verification failure uncovers a real
  defect; such a defect gets a new corrective rather than being hidden in M017.
- M011's amended single-router qualification basis remains explicit: Java I2P is fully
  live-qualified; i2pd is partial; i2pr is not live-qualified by this repository yet.
- “Foundation closed” must not be rewritten as “multi-router interoperability proven.”
- Historical blocked/not-run artifacts remain preserved.
- Historical M002–M006 conditional closure records are not rewritten.
- M012–M016 closure records remain immutable except for purely mechanical broken links if
  discovered.
- Public API snapshot remains stable unless verification exposes accidental drift.
- Exact-head CI must be green before merge.
- `main` is the final authority after merge, not a planning branch.
- Branch deletion occurs only after ancestry/integration is proven.
- No crate publication occurs.
- Repository license selection is outside M017; lack of a license blocks publication, not
  pre-1.0 implementation work.

## 5. Scope

### In scope

- README current-status reconciliation;
- M011 plan status/current-authority reconciliation;
- additive M011 closure metadata correction;
- current conformance-summary regeneration;
- explicit Java/i2pd/i2pr qualification-scope wording;
- API snapshot comparison and promotion from informational to blocking if the snapshot
  matches current code;
- exact-head local verification;
- exact-head hosted CI;
- correct PR/merge vehicle;
- stale PR disposition;
- merge to `main`;
- post-merge CI;
- planning registry/roadmap closure;
- obsolete branch cleanup;
- successor readiness audit for M007/M008.

### Explicitly out of scope

- new SAM commands/styles;
- another live-router compatibility campaign;
- requiring i2pd payload success to preserve M011 closure;
- requiring i2pr live qualification to preserve M011 closure;
- C ABI/Python implementation;
- service-tunnel adapter implementation;
- selecting or publishing under a repository license;
- changing MSRV;
- renumbering historical milestones.

## 6. Required changes

### A. Reconcile current documentation

Update README to state the actual current foundation:

- strict Rust foundation implementation is closed;
- Java I2P full live matrix passed 11/11;
- i2pd produced useful compatibility evidence but no successful payload row;
- i2pr remains an unqualified future compatibility target in this repo;
- no cross-router/multi-router interoperability claim is made;
- live GitHub Actions were intentionally removed because no self-hosted runner is
  registered;
- normal hosted CI covers deterministic/platform verification.

### B. Reconcile M011 plan authority

Change the M011 plan's current status from `active` to `closed`.

Preserve the historical research/attempt narrative, but make the current authority obvious.
Either:

- add a short current-authority section near the top pointing to the final M011 closure and
  Java artifact; or
- clearly mark the now-satisfied “Full closure evidence still required” section as
  historical/pre-closure context.

Do not delete the i2pd attempt history.

### C. Correct M011 closure metadata additively

The original M011 closure was created in the same commit as the final implementation
changes. It therefore cannot truthfully name an earlier implementation commit for the
complete final state.

Add a concise M017 reconciliation note to `011-status.md` stating:

- final integrated implementation/evidence/closure revision:
  `de6bd3258105e6694ded1fdc5ef81abccd2d02fe`;
- parent `4c15aa02a30383a79cd2601ad589e73fdf62cb09` is the last pre-final-fix commit;
- the “uncommitted working tree” wording described work that was subsequently committed in
  `de6bd3258105e6694ded1fdc5ef81abccd2d02fe`;
- no live result or acceptance conclusion is changed by the correction.

Do not fabricate a separate implementation SHA that never existed.

### D. Reconcile conformance observations

Preserve the original all-not-run observation as history by either:

- moving/copying it to an explicitly dated historical artifact path; or
- retaining it in the live qualification narrative/artifact archive.

Make `specs/conformance-observations.csv` a truthful current compact summary derived from
the final evidence.

At minimum it must distinguish:

**Java I2P**

- full 11/11 live pass at the selected runtime/package version;
- SAM 3.3;
- exact operations reflected by the final matrix.

**i2pd**

- HELLO/session/identity/compatibility observations;
- PRIMARY explicit unsupported behavior;
- payload qualification incomplete/not-run;
- no client failure claim from router crashes/readiness limitations.

**i2pr**

- not live-qualified by M011;
- no interoperability claim.

Do not collapse “not tested”, “router unavailable”, “unsupported”, and “failed” into one
state.

### E. API snapshot hardening

The CI workflow currently treats the bare API snapshot comparison as informational.

At the M017 implementation head:

1. run the snapshot checker;
2. if `api/public-types.txt` matches the intended 347-declaration current surface, make
   the comparison a required CI step;
3. if it does not match, reconcile the snapshot in the same commit and document the diff;
4. retain mutation/self-tests as blocking.

After M017 closure, ordinary CI must fail on unregistered public API drift.

### F. Exact-head verification

Before opening/updating the final merge PR, run at least:

```bash
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
cargo +1.89 check --locked --workspace --all-targets
cargo +1.89 test --locked --workspace --all-targets
python3 -m unittest discover -s tests -p 'test_*.py'
python3 scripts/check-api-snapshot.py
python3 scripts/check-api-snapshot.py --self-test
python3 scripts/check-proto-boundary.py
python3 scripts/check-proto-boundary.py --self-test
python3 scripts/check-conformance-artifact.py --self-test
git diff --check
```

Also validate the final Java conformance artifact and current summary.

Only commands actually executed may appear as passed in the M017 closure.

### G. Correct merge vehicle

Do not merge PR #1 as the complete foundation.

Preferred sequence:

1. create a new PR from the M017 implementation branch to `main`, covering the entire
   foundation history;
2. reference/supersede PR #1 and close it without merge once the replacement PR exists;
3. ensure the replacement PR's head is the exact M017 candidate SHA;
4. wait for all required hosted CI jobs on that exact SHA;
5. merge only after they are green.

If GitHub safely permits retargeting/updating PR #1 to the complete M017 branch without
losing review clarity, that is acceptable, but the final PR must visibly contain all 23+
foundation commits and M017 reconciliation. A new PR is preferred because PR #1's title
and head describe only M005/M006.

### H. Post-merge main verification

After merge:

- fetch `main`;
- prove it contains the final M017 tree;
- record merge SHA;
- require normal `push: main` CI to pass;
- verify registry/README/roadmap links on `main`;
- verify the Java artifact and API snapshot are present unchanged.

M017 is not closed merely because the PR merge button succeeded.

### I. Planning branch cleanup

After post-merge verification, compare each branch to `main` and delete only when its
tip is an ancestor of or fully represented by `main`:

- `plans/001-004-sam-foundation`;
- `plans/005-006-foundation-correctives`;
- `plans/012-013-foundation-qualification-correctives`;
- M017's own planning/implementation branch after closure if repository convention permits.

Do not delete a branch with unique commits.

Record branch disposition in the closure.

## 7. Ordered work packages

### WP A — documentation/evidence truth

Reconcile README, M011 plan/current authority, M011 closure metadata, conformance summary,
registry, and roadmap.

Exit: no current document says M011 is still open or that the final matrix has no passes.

### WP B — API/verification hardening

Run the full floor and make current API snapshot drift blocking.

Exit: exact local candidate passes; no known unregistered API drift remains.

### WP C — final merge PR

Create/retarget the correct complete-foundation PR; supersede stale PR #1.

Exit: one unambiguous PR contains the final candidate and required CI is green at its exact
head.

### WP D — merge and main CI

Merge to `main` and require post-merge CI.

Exit: `main` is canonical and green.

### WP E — branch/registry closure

Retire superseded branches only after ancestry proof; write M017 closure and successor
readiness.

Exit: no stale branch/PR competes with `main` as foundation authority.

## 8. Failure, cancellation, restart, and contention semantics

- If exact-head tests fail, stop merge and register/fix the defect before proceeding.
- If API snapshot differs unexpectedly, stop and classify each change; do not regenerate
  blindly.
- If another commit lands on the candidate branch after CI, CI must rerun on the new exact
  head.
- If `main` changes concurrently, update/rebase/merge as appropriate and rerun the full
  candidate verification.
- If post-merge `main` CI fails, M017 remains open even if the PR is merged.
- Branch deletion is last and is skipped on any ancestry ambiguity.
- Closing stale PR #1 must not delete its branch before the final merge is secure.

## 9. Compatibility and migration

No intended Rust API or wire compatibility change.

If reconciliation reveals an accidental public API delta, classify it explicitly and
update `docs/api-migration.md`; otherwise M017 should have a zero-semantic-change API
diff.

The M011 validation basis remains:

- strong semantic live evidence against Java I2P;
- partial i2pd compatibility evidence;
- no i2pr live evidence;
- no cross-router claim.

## 10. Required tests

- full Rust workspace floor;
- Rust 1.89 floor;
- Python harness suite;
- API snapshot comparison + self-test;
- protocol-boundary comparison + self-test;
- conformance artifact validator self-test;
- final Java artifact validation;
- current conformance-summary consistency check;
- stale-status text check for README/M011;
- tracked Python cache guard;
- exact-head hosted CI;
- post-merge main CI.

A small static reconciliation checker is encouraged if it cheaply catches contradictory
status phrases such as “M011 remains open” after registry closure.

## 11. Required verification commands

In addition to Section 6F, inspect repository state with commands equivalent to:

```bash
git log --oneline main..HEAD
git diff --stat main...HEAD
git branch --contains <final-candidate-sha>
git merge-base --is-ancestor <candidate-sha> main
git ls-files | grep -E '(^|/)(__pycache__/|.*\.py[co]$)' && exit 1 || true
```

The GitHub-side implementation must also record:

- final PR number;
- exact PR head SHA;
- exact PR CI run IDs/conclusions;
- merge SHA;
- exact post-merge `main` CI run IDs/conclusions;
- stale PR #1 disposition;
- deleted/retained branch list.

## 12. Documentation updates

Required current-authority files:

- `README.md`;
- `plans/implementation/sam-library/011-live-router-payload-qualification.md`;
- `plans/closure/sam-library/011-status.md` — additive metadata correction only;
- `specs/conformance-observations.csv`;
- `specs/live-router-qualification.md` if current summary wording needs alignment;
- `plans/subsystems/sam-library-roadmap.md`;
- `plans/registry.md`;
- `plans/closure/sam-library/017-status.md`.

Historical live artifacts remain preserved.

## 13. Acceptance criteria

M017 closes only when:

1. README reports the Java 11/11 final live result and no longer says M011 is open.
2. M011 plan current status is closed.
3. M011 closure metadata identifies `de6bd3258105e6694ded1fdc5ef81abccd2d02fe` as the final integrated
   implementation/evidence/closure revision without pretending an earlier standalone
   implementation SHA exists.
4. `specs/conformance-observations.csv` reflects current Java/i2pd/i2pr qualification
   truth while old blocked observations remain archived/preserved.
5. Current docs explicitly distinguish single-router Java qualification from multi-router
   interoperability.
6. API snapshot comparison is blocking and passes at the candidate head.
7. Full local stable + MSRV verification passes.
8. Hosted Linux/MSRV/macOS/Windows CI passes at the exact final PR head.
9. PR #1 is superseded/closed or accurately retargeted; it is not accidentally merged as
   the complete foundation while missing later work.
10. One complete foundation PR is merged to `main`.
11. Post-merge `main` CI passes.
12. `main` contains the final Java artifact, current planning registry, and reconciled
    docs.
13. Superseded planning branches are deleted only after ancestry/integration proof.
14. M017 closure records exact candidate/merge SHAs, CI runs, PR disposition, and branch
    cleanup.
15. No SAM protocol/runtime behavior was changed by reconciliation unless separately
    registered as a corrective.

## 14. Stop conditions

Stop and register a new corrective rather than stretching M017 if:

- exact-head hosted CI exposes a platform-specific runtime defect;
- the final Java artifact fails current validation;
- API snapshot reconciliation reveals an unintended semantic break;
- any supposedly superseded branch contains unique implementation work;
- post-merge main differs materially from the tested PR head;
- stale documentation reflects an unresolved technical disagreement rather than simple
  lag.

## 15. Closure evidence required

The M017 closure must contain:

- implementation/reconciliation SHA;
- final PR number/head;
- all local commands actually run;
- exact hosted PR CI runs;
- final Java artifact validation result;
- API declaration count/diff;
- current qualification summary for Java/i2pd/i2pr;
- stale PR #1 disposition;
- merge SHA;
- post-merge main CI runs;
- branch ancestry/deletion table;
- explicit no-new-capability statement;
- successor readiness decision.

## 16. Successor decision

After M017 closes on `main`:

- **M007 C ABI + Python bindings** may be researched/planned and implemented against the
  merged pre-1.0 Rust foundation. Repository license selection remains required before
  public package/wheel distribution, but is not itself an implementation blocker.
- **M008 i2pr service-tunnel adapter** remains blocked until the selected
  `i2pr-service-tunnels` revision is stably merged/consumable from i2pr.
- broader i2pd/i2pr/multi-router compatibility hardening remains future maturity work, not
  a reason to reopen this foundation closure.

M017 is intended to be the last foundation-only milestone before downstream API surfaces
are planned.
