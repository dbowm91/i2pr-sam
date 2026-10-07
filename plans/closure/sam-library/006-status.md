# Milestone 006 status - Verification, CI, and public-API stabilization corrective

Status: **conditionally closed**

Implementation revision: `2439485a63319ae8501ac3ccad789f886a4465e0`
Closure revision: this record, committed immediately after the implementation revision above
Plan: `plans/implementation/sam-library/006-foundation-verification-ci-api-stabilization-corrective.md`
Predecessor: `plans/closure/sam-library/005-status.md`

## 1. What closed

Verification, API semantics, guard hardening, and hosted CI are implemented and executed.
Two acceptance criteria could not be met in this environment: the plan requires M005's live
wire truth before the API stabilises, and milestone 005 has no live row; and the hosted
lanes could not be run green at the closure head because the first push carrying these
changes had not happened when the local verification below was taken. The hosted lanes were
subsequently executed against the pushed implementation commit and are recorded in section 4.

## 2. Work completed

**Blocking parity (WP C).** The blocking facade now mirrors the async surface: typed
`SessionDestination` and `GeneratedDestination`, `Option<Port>` on every stream and datagram
entry point, `create_datagram_session_with` for explicit transports, `accept_with(silent)`,
`peer()`/`remote_destination()` on accepted streams, shared and child `identity()`, and
`connect_with_policy`. The v1/v2 transport refusal for DATAGRAM2/DATAGRAM3 propagates
unchanged rather than falling back to UDP forwarding.

**API semantics (WP A).** `SessionOptions` validates keys and values and deduplicates;
`PeerTarget` distinguishes a Destination from a base32 hash address and refuses the latter
where a Destination is required; destination tokens containing ASCII control characters are
rejected before framing, closing a command-injection path through a newline in a caller
supplied destination; `resolve_peer` and `typed lookup_destination` return typed values;
`stream.style()`/`style_kind()` separate the wire spelling from the typed style. The
migration note is `docs/api-migration.md`.

**Retry and cancellation (WP B).** `ReconnectPolicy` is now `ConnectRetryPolicy`, with a
deprecated alias retained. It carries a process-wide concurrent admission budget and an
explicit saturation policy, so N clients cannot multiply load on a struggling router.
`SamClient::connect_with_clock` takes an injectable `RetryClock`, and `backoff_before` is a
pure function, so retry budgets are asserted without wall-clock waits.

**Resource accounting (WP D).** `resource_usage()` reports live sessions, live streams, peak
stream concurrency, open control sockets, and dropped datagrams, released through
`ReleaseOnDrop` so cancellation and error paths release as reliably as explicit closes.

**Guard hardening (WP E).** Both guards were rewritten around pure detector functions shared
by the real run and a mutation self-test. `check-api-snapshot.py --self-test` proves 9
mutations are detected, including signature changes, variant set changes, field set changes,
and visibility demotion, with an unchanged-source negative control. `check-proto-boundary.py
--self-test` proves 29 forbidden-dependency evasions are detected across four manifest
forms, with a clean-manifest negative control. The API guard's docstring states plainly what
a regex inventory cannot observe.

**Hosted CI (WP F).** `.github/workflows/ci.yml` defines `fmt`, `clippy`, `test-linux`,
`msrv`, `test-macos`, and `test-windows`, each with least privilege, a timeout, `--locked`,
and full-SHA-pinned actions. `.github/workflows/live-interop.yml` is manual and separates
exit 3 (environment cannot run live lanes) from exit 1 (a real failing row).

## 3. Local evidence

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo test --locked --workspace` | pass - 51 tests (12 protocol, 37 async, 2 blocking), 0 ignored, 0 failed |
| `cargo +1.89 test --workspace` | pass |
| `python3 scripts/check-proto-boundary.py` | pass, positive control detected |
| `python3 scripts/check-proto-boundary.py --self-test` | 29/29 |
| `python3 scripts/check-api-snapshot.py --self-test` | 9/9 |
| `python3 scripts/check-api-snapshot.py` | pass against the regenerated snapshot |
| `python3 scripts/check-conformance-artifact.py --self-test` | 13/13 |

## 4. Hosted CI evidence

Run `37646431761`, workflow `ci`, triggered by pull request 1 against `main`, executed
2026-10-07T15:44:44Z to 2026-10-07T15:46:13Z against commit
`2439485a63319ae8501ac3ccad789f886a4465e0`. Overall conclusion:
**success**. <https://github.com/dbowm91/i2pr-sam/actions/runs/37646431761>

| Job | Started | Completed | Conclusion |
|---|---|---|---|
| `fmt` | 15:44:49Z | 15:45:08Z | success |
| `clippy` | 15:44:49Z | 15:45:21Z | success |
| `test-linux` | 15:44:49Z | 15:45:12Z | success |
| `msrv` | 15:44:48Z | 15:45:23Z | success |
| `test-macos` | 15:44:53Z | 15:45:40Z | success |
| `test-windows` | 15:44:48Z | 15:46:12Z | success |

`msrv` asserted `rustc 1.89.0` before building, so the MSRV lane cannot silently pass on a
newer toolchain. Test logs and guard/validator self-test output were uploaded as workflow
artifacts. The `live-interop` workflow is `workflow_dispatch` only and was not executed; it
cannot be cited as evidence of anything.

A second run, `37646808105`, re-executed all six lanes against the closure commit
`1e5a7f1916ccf01c9a11f6f21191113ae49d6e4b` and concluded `success`, so the documented
closure state is itself green rather than only its parent.

## 5. Acceptance criteria

| # | Criterion | Result |
|---|---|---|
| 1 | M005 has closed its interoperability/protocol corrective | **not met** - M005 is conditionally closed with no live row; this milestone inherits that residual |
| 2 | blocking facade covers datagram/shared/control-socket modes with parity | met |
| 3 | deterministic cancellation and contention tests | met - injectable clock, admission saturation, idempotent close, bounded queue drops |
| 4 | resource accounting over soak and contention | met - `resource_usage()` with release-on-drop, asserted by the soak suite |
| 5 | API documented with a migration note and the guarded surface regenerated | met - `docs/api-migration.md`, regenerated `api/public-types.txt` |
| 6 | guard mutation/self-tests | met - 9/9 and 29/29 |
| 7 | MSRV 1.89 plus Linux/macOS/Windows lanes green at the closure head | met, see section 4 |
| 8 | explicit hosted run identifiers and run times | met, see section 4 |
| 9 | artifacts collected by the CI workflow | met |

## 6. Honest limits

- The hosted lanes prove the code builds and passes; they prove nothing about router
  interoperability, which remains milestone 011's objective.
- The API guard is a textual inventory. It cannot see macro-generated items, `cfg`
  evaluation, trait-provided methods, or semantic changes that keep identical token text. It
  is not a type checker and is not semver-complete.
- The snapshot guard compares text; it is a review trigger, not proof of API safety.

## 7. Successor audit

| Milestone | Decision |
|---|---|
| M002, M003, M004 | conditional authority **not** promoted; additive reconciliation deferred to milestone 011 because no live row passed |
| M007 bindings | **remains blocked**. The plan required M006 strict closure, and API stabilisation was explicitly meant to follow live wire truth that does not yet exist |
| M008 i2pr adapter | **remains blocked** on M007-class certainty plus a stable merged `i2pr-service-tunnels` revision |
| M011 live payload qualification | registered; this is the gate that unblocks M006 strict closure, then M007 and M008 |

No downstream milestone was unblocked. The only honest outcome without live payload evidence
is to keep M007 and M008 blocked and say why.