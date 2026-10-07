# SAM Library Milestone 004 — Runtime facades, resilience, conformance, and API stabilization

Status: conditionally closed

Repository baseline: `956238fcced742836833befaf6e748e97f435001` (M003 implementation baseline).

Source roadmap:

- `plans/subsystems/sam-library-roadmap.md#004--runtime-facades-resilience-conformance-and-api-stabilization`

Primary class: infrastructure + polish + invariant

## 1. Objective

Close the Rust foundation by making the proven async client practical and stable:
introduce a blocking facade, explicit reconnect/recovery policy, deterministic fault
harness, long-running lifecycle/resource tests, API review/snapshot, and a repeatable
multi-router conformance command.

This milestone is the gate before C/Python bindings or the service-tunnel adapter are
planned for implementation.

## 2. Why this milestone is ready

Blocked until M003 closes with:

- ordinary STREAM;
- all supported datagram styles;
- shared-session lifecycle;
- Java/i2pd/i2pr compatibility matrix;
- centralized capability/quirk model.

M004 must not paper over an unresolved M003 protocol mismatch with retry logic.

## 3. Current implementation evidence

Refresh from M003 closure. The expected client is async-first and has explicit ownership,
but not yet a supported blocking API or release-stability contract.

## 4. Invariants that must not regress

- blocking facade delegates to canonical async semantics;
- no second parser/session implementation;
- automatic recovery never changes Destination identity silently;
- transient Destination sessions are not transparently recreated as “the same” identity;
- persistent private Destinations may be recreated only under explicit policy;
- retries/backoff are bounded, jittered where appropriate, and cancellation-aware;
- no reconnect storm;
- capability cache cannot override contradictory live evidence indefinitely;
- deterministic test harness remains separate from production bridge code;
- public API snapshot contains no secret-bearing Debug leakage or raw router-quirk strings
  where semantic types exist.

## 5. Scope

### In scope

- blocking Rust crate/facade;
- explicit reconnect policy and retry classifier;
- bridge/session health semantics;
- deterministic fake SAM bridge/fault injector;
- lifecycle soak tests under bounded duration;
- connection/session resource accounting;
- compatibility/conformance runner for live routers;
- public API review and declaration snapshot;
- MSRV/stable CI;
- examples covering async and blocking use;
- package metadata/readme/docs needed for a later release.

### Explicitly out of scope

- C ABI;
- Python;
- service-tunnel adapter;
- daemon;
- WebUI/CLI product;
- automatic daemon discovery;
- package publication if repository license remains unselected;
- hidden forever-retry behavior.

## 6. Required production changes

### Blocking facade

Prefer a separate crate such as `i2pr-sam-blocking` so blocking/runtime ownership does
not pollute the async library.

The facade may own a private current-thread Tokio runtime or equivalent implementation,
but:

- it must not be callable from contexts where that strategy would deadlock without a
  clear typed error/documented restriction;
- it must preserve close/cancel/error semantics;
- stream handles implement standard `Read`/`Write`;
- blocking listener/datagram operations have explicit timeouts/cancellation strategy.

### Recovery model

Define `ReconnectPolicy` or equivalent.

Default should be no silent identity-changing recovery.

Classify failures:

- protocol permanent;
- capability unsupported;
- auth/config permanent;
- transient bridge transport;
- transient I2P/router result;
- cancellation/user close.

Only explicitly selected transient classes may retry.

Persistent Destination material may be reused to reconstruct a session when caller policy
allows. Transient identity recreation requires explicit opt-in and must report identity
change.

### Fault harness

Add a deterministic bridge test harness capable of:

- partial lines;
- split replies;
- delayed replies;
- early EOF;
- malformed reply;
- wrong ID/result;
- control close during child operations;
- dropped/reordered local datagram forwarding where applicable;
- stalled reads/writes;
- connection refusal/recovery.

Use it to verify cleanup/backoff rather than sleeping on real wall time where possible.

### Conformance runner

Provide a developer/test command, not necessarily a shipped end-user CLI, that runs the
feature matrix against a configured bridge and emits structured JSON plus human summary.

It must not claim unsupported rows as pass. Output includes router label/version supplied
by the harness, negotiated version, observed capabilities, dialect, and failures.

### API stabilization

Review all public items and create a snapshot or semver-aware guard.

Public API should center semantic types and hide:

- raw command strings;
- internal connection IDs;
- Tokio task handles;
- router-brand checks;
- mutable capability internals.

## 7. Ordered work packages

### WP A — deterministic fault bridge

Build harness before retry logic so recovery is test-driven.

### WP B — recovery policy

Implement explicit classifier/backoff/reconnect semantics and identity rules.

### WP C — blocking facade

Wrap the canonical async client and prove parity for representative STREAM/datagram/shared
flows.

### WP D — resource/soak qualification

Repeated create/use/close cycles, cancellation races, queue pressure, and bridge restarts
with stable handle/task/socket counts.

### WP E — conformance runner

Generate durable JSON matrix for Java/i2pd/i2pr.

### WP F — API review/snapshot/docs

Freeze the first foundation public surface and document breaking-change policy for
pre-1.0 development.

## 8. Failure, cancellation, restart, and contention semantics

- retries stop at configured attempt/time budget;
- cancellation interrupts backoff promptly;
- close outranks retry;
- one session recovery cannot monopolize the global connector;
- concurrent recoveries have a named global limit;
- persistent shared-session recreation restores children only under explicit policy and
  fails the generation atomically if required children cannot be restored;
- callers can observe that a session generation changed;
- blocking facade propagates cancellation/timeout without abandoning async tasks.

## 9. Compatibility and migration

Still pre-1.0, but M004 creates the first reviewed API baseline.

Breaking changes after M004 require:

- plan-of-record;
- snapshot diff;
- migration note;
- evidence that the old shape is materially inadequate.

Router compatibility data remains advisory/observed, never an excuse to bypass live
protocol errors.

## 10. Required tests

- blocking parity for STREAM;
- blocking parity for each supported datagram style;
- blocking shared-session child use;
- no-runtime/deadlock negative cases;
- transport retry success/exhaustion;
- auth/protocol errors never retried;
- cancellation during backoff;
- identity-preserving persistent recovery;
- transient identity-change opt-in only;
- bridge restart;
- fault harness partial/malformed/stall cases;
- hundreds of bounded lifecycle iterations with no task/socket/session growth;
- API snapshot positive/negative control;
- conformance JSON schema and truthful unsupported rows;
- MSRV 1.89 and stable.

## 11. Required verification commands

M003 floor plus blocking/fault/soak/conformance/API checks. At minimum:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
```

Live conformance commands must be recorded with exact router pins/environment.

## 12. Documentation updates

- async vs blocking usage;
- reconnect/identity semantics;
- capability/quirk model;
- compatibility matrix;
- API stability policy;
- performance/resource limits;
- roadmap/registry/closure;
- readiness notes for future bindings and i2pr service-tunnel adapter.

## 13. Acceptance criteria

1. Blocking API is a facade, not a second implementation.
2. Retry/reconnect is explicit, bounded, and identity-safe.
3. Fault harness deterministically covers transport/control failures.
4. Repeated lifecycle tests show no unbounded resource growth.
5. Live conformance runner emits truthful structured results.
6. Public API snapshot is reviewed and guarded.
7. Java/i2pd/i2pr foundation matrix is refreshed.
8. MSRV and stable verification pass.
9. No C/Python/daemon capability is claimed.
10. Foundation is documented as ready for separate binding/adapter milestone planning.

## 14. Stop conditions

Stop if:

- blocking support requires duplicating protocol/session logic;
- recovery semantics cannot preserve identity truthfully;
- M003 still has unresolved router semantic divergence;
- soak tests show unexplained task/socket/session growth;
- public API cannot be stabilized without first redesigning core ownership;
- publication is requested before a repository license is selected.

## 15. Closure evidence required

Blocking parity matrix, retry classifier table, fault cases, soak counts/resource deltas,
API snapshot diff, MSRV/stable commands, live conformance JSON artifacts, exact router
versions, and explicit successor readiness for bindings/service-tunnel adapter.

## 16. Handoff notes

After M004 closure, research M005/M006 separately. Do not automatically implement FFI or
the tunnel adapter in the same change; both create new compatibility surfaces deserving
their own plans.

Closure evidence: `plans/closure/sam-library/004-status.md`. Multi-router conformance and
broader recovery qualification remain explicit residual conditions.
