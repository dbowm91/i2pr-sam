# SAM Library Milestone 006 — Verification, CI, and public-API stabilization corrective

Status: blocked on Milestone 005

Repository baseline: `d7ee20fff63ba4421e957455c10d94c23d4529aa` planning baseline; refresh after M005 closure.

Corrects:

- verification and API-stability gaps in `plans/closure/sam-library/004-status.md`
- test-surface gaps carried from M002/M003
- absence of hosted CI on the implementation branch

Source roadmap:

- `plans/subsystems/sam-library-roadmap.md`

Long-term requirements:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`

Applicable ADRs:

- `plans/adrs/ADR-0001-clean-room-reference-and-provenance-policy.md`
- `plans/adrs/ADR-0002-layered-client-runtime-and-bindings-ownership.md`

Primary class: invariant + infrastructure + polish

## 1. Objective

Make the live-corrected Rust foundation safe to freeze as the substrate for future C/Python
bindings and the i2pr service-tunnel adapter.

M006 closes the engineering-quality gaps that M004 left conditional:

- blocking facade parity is tested for the entire supported async surface;
- cancellation, contention, close, queue pressure, and retry behavior are systematically
  qualified;
- misleading or stringly public identity/session APIs are corrected before FFI hardens
  them;
- the retry surface says what it actually does;
- API/boundary guards have mutation/self-tests rather than only happy-path snapshots;
- clean hosted CI covers supported host families and MSRV;
- resource/lifecycle testing moves beyond a single 100-cycle STREAM control-socket test.

This plan does not add foreign-language bindings.

## 2. Why this milestone is ready

Blocked on M005 because public API stabilization must follow live interoperability, not
precede it.

M004 closure already identifies the remaining defects:

- only 18 stable workspace tests at closure;
- blocking STREAM parity exists but blocking datagram/shared parity does not;
- recovery is initial connection retry only, despite the public type being named
  `ReconnectPolicy`;
- no global concurrent retry/recovery limit;
- no deterministic clock/backoff qualification;
- no live router matrix;
- no shared-session payload soak;
- no external resource-count measurement;
- API snapshot is declaration-based rather than a semantic compatibility tool;
- no hosted workflow exists.

Additionally, the current public surface still exposes many semantic identity/session
values as `String` or `&str` even though typed `Destination`, `SecretDestination`,
`SessionId`, and port/protocol types already exist.

## 3. Current implementation evidence

Expected M005 input:

- corrected live-qualified wire semantics;
- concrete Destination handling;
- ordinary control-socket datagram disposition;
- truthful router capability matrix;
- deterministic regressions for live defects.

Current M004 baseline includes:

- Tokio async client;
- blocking facade;
- bounded initial connection retry;
- deterministic mock bridge tests;
- public declaration snapshot;
- protocol boundary checker;
- local stable + Rust 1.89 command evidence.

## 4. Invariants that must not regress

- blocking facade remains a wrapper over one canonical async implementation;
- no second protocol parser/session state machine appears;
- retry never changes an established Destination identity silently;
- initial-connect retry and established-session recovery are separate concepts;
- public API never turns an unverified D3 hash into authenticated identity;
- close/cancel is idempotent and releases owned resources once;
- no detached task/socket remains after owned object teardown;
- protocol crate stays runtime-neutral;
- API stabilization does not expose Tokio task handles, router-brand conditionals, or raw
  command assembly;
- CI cannot silently skip a required unit/static lane because a tool is missing.

## 5. Scope

### In scope

- public Rust API audit and pre-1.0 cleanup;
- semantic identity/session typing;
- retry naming/semantics correction;
- blocking facade parity for all supported operations;
- cancellation/close/contention race tests;
- bounded queue and resource-pressure tests;
- deterministic time/backoff tests;
- resource-accounting soak;
- API snapshot self-test/mutation test;
- protocol-boundary guard self-test;
- Linux/macOS/Windows hosted CI where supported;
- Rust 1.89 MSRV lane;
- stable lint/doc/test floor;
- optional live-interoperability workflow reuse from M005;
- docs/registry/closure reconciliation.

### Explicitly out of scope

- C ABI;
- Python extension;
- service-tunnel adapter;
- daemon/reload/config persistence;
- automatic established-session recovery unless separately justified;
- publication;
- changing router-side protocols.

## 6. Required production changes

### Public semantic API audit

Review every public declaration in all three crates before freezing the next snapshot.

At minimum disposition:

- `generate_destination() -> (String, SecretDestination)`;
- `lookup() -> String`;
- `remote_destination() -> Option<&str>`;
- shared/session `destination() -> &str`;
- session IDs accepted/returned as `&str`;
- raw optional u16 ports where `Port` already exists;
- duplicate `SecretDestination` wrappers between crates if both exist;
- raw option lists `&[(String, String)]` for fields with known reserved semantics.

The goal is not maximal type complexity. It is to ensure identity, secret material, session
identity, and trust-sensitive metadata are not accidentally flattened into strings before
FFI.

A reasonable target is:

- typed public Destination/SecretDestination results;
- typed SessionId at ownership boundaries;
- typed peer identity;
- clearly named target/reference type where hostnames/b32/base64 Destination are all
  valid and cannot simply be represented by `Destination`;
- arbitrary passthrough options isolated behind a builder/validated option bag that
  prevents reserved-key duplication.

Breaking pre-1.0 changes are allowed here and should be preferred over compatibility debt.

### Retry semantic correction

The current `ReconnectPolicy` applies to creating the initial `SamClient` connection;
it does not recover established sessions.

Choose one truthful design:

A. rename/narrow it to `ConnectRetryPolicy` (preferred unless M005 proves a real
   requirement for broader recovery), or
B. implement actual established-session recovery with explicit persistent identity,
   generation, child restoration, and cancellation semantics.

Do not retain a name that implies behavior the type does not provide.

For initial connect retries:

- bound total attempts;
- bound elapsed time;
- bound backoff;
- use deterministic time in tests where practical;
- bound concurrent retrying connectors globally or through a documented per-client
  admission mechanism;
- cancellation/close interrupts backoff promptly;
- permanent auth/config/protocol errors are never retried.

### Blocking parity

Every supported async capability must have either:

- a blocking wrapper + parity test; or
- an explicit “async only” documented exclusion with architectural reason.

Parity test matrix includes:

- Destination generation;
- naming lookup;
- ordinary STREAM connect/accept;
- ordinary supported datagram styles/modes;
- shared owner create;
- child add/remove;
- shared STREAM child payload;
- shared datagram child payload;
- close/cancel;
- timeout behavior;
- unsupported capability result;
- nested-runtime rejection.

### Cancellation and contention

Add deterministic tests for:

- cancel during HELLO;
- cancel during STREAM CONNECT;
- cancel pending ACCEPT;
- two or more concurrent ACCEPTs where protocol version permits;
- close racing with connect/accept admission;
- close racing with UDP/control-socket receive;
- shared owner close racing with child add/remove;
- duplicate child/listener tuple under concurrent callers;
- queue/backpressure saturation;
- client connection retry cancellation;
- blocking timeout cleanup.

Tests must assert no orphan operation remains.

### Resource accounting and soak

A single “socket reaches EOF after 100 cycles” test is insufficient.

Add test-visible resource accounting/harness instrumentation capable of detecting growth in:

- live bridge connections;
- session control connections;
- active stream operation handles;
- shared owners/children;
- UDP sockets;
- spawned owned tasks if any;
- queued datagram bytes/messages;
- retrying connectors.

Run bounded repeated cycles across:

- STREAM;
- ordinary datagram;
- shared STREAM+datagram;
- cancellation failure paths;
- bridge restart/failure.

Use deterministic counters where OS-level descriptor enumeration is not portable. Platform
specific descriptor checks may supplement but not replace ownership counters.

### Guard hardening

`check-api-snapshot.py` must gain a self-test or mutation fixture proving it detects:

- removed public item;
- changed function signature;
- changed enum variant set;
- changed public field surface where intentionally tracked.

`check-proto-boundary.py` keeps its positive synthetic runtime-dependency control and is
run in CI.

If snapshot tooling cannot correctly model Rust syntax, use a more robust semver/public API
tool or parse rustdoc JSON. Do not claim semantic compatibility from a regex that cannot
observe it.

### Hosted CI

Add `.github/workflows/ci.yml` or equivalent with a fail-closed baseline.

Required coverage:

- Linux stable: fmt/check/test/clippy/docs/guards;
- Rust 1.89 MSRV check/test/clippy on Linux;
- macOS stable check/test at minimum;
- Windows stable check/test at minimum;
- API snapshot guard;
- protocol boundary guard;
- JSON/schema/conformance artifact validation.

Network mock tests must bind ephemeral loopback ports, not fixed ports.

Live router qualification may be a separate manual/scheduled workflow. It must not make
ordinary CI depend on external public I2P availability.

### Test decomposition

Do not target an arbitrary coverage percentage. Instead create a traceable matrix from
public capability/critical invariant to at least one positive and one relevant negative
test.

The closure record must enumerate the matrix.

## 7. Ordered work packages

### WP A — API semantic correction

Fix high-risk stringly/misleading types and regenerate API baseline.

Acceptance:

- identity/trust-sensitive APIs are typed or explicitly justified;
- pre-1.0 migration note documents breakage.

### WP B — retry/lifecycle semantics

Rename or implement retry semantics truthfully and add deterministic cancellation/global
admission tests.

Acceptance:

- no API claims established-session recovery unless it actually exists.

### WP C — blocking parity

Fill the datagram/shared/blocking gaps.

Acceptance:

- parity matrix covers every supported async capability.

### WP D — concurrency/resource hardening

Implement race, pressure, and soak tests with test-visible ownership accounting.

Acceptance:

- repeated mixed lifecycle cycles show zero retained owned resources after teardown.

### WP E — guard hardening

Add mutation/self-tests for API and boundary guards.

Acceptance:

- known synthetic breakages cause non-zero guard exits.

### WP F — hosted CI

Add and execute the cross-platform/MSRV workflow.

Acceptance:

- required hosted jobs are green at the exact closure head.

### WP G — foundation strict closure

Reconcile M002–M004 conditional authority based on M005/M006 evidence.

Acceptance:

- roadmap no longer says foundation is closed unless every remaining condition is either
  satisfied or explicitly deferred outside the foundation with owner approval.

## 8. Failure, cancellation, restart, and contention semantics

- `close` wins over new operation admission once closure begins.
- waiters wake with Closed/Cancelled, not arbitrary I/O errors produced by abandoned tasks.
- concurrent child/session admission is bounded before allocation.
- retry/backoff is interruptible.
- blocking timeout does not detach the underlying async future/task.
- an async operation may not be driven simultaneously by two blocking wrappers.
- resource counters are decremented on every error path, not only normal Drop.
- test harness failures perform bounded cleanup before returning.

## 9. Compatibility and migration

This is the final intended breaking-API window before C/Python design.

Document all changes from the M004 snapshot. No compatibility shim is required for an
unpublished pre-1.0 API unless it materially improves downstream migration.

M007 bindings must use the post-M006 surface only.

## 10. Required tests

### Public API

- typed Destination return/peer identity;
- typed session identity/target validation;
- reserved option rejection;
- API snapshot mutation self-test.

### Blocking parity

- ordinary STREAM;
- ordinary D1/RAW and supported D2/D3 modes;
- shared STREAM child;
- shared datagram child;
- add/remove/close;
- unsupported capability;
- timeout;
- nested runtime.

### Cancellation/contention

- HELLO cancellation;
- CONNECT cancellation;
- ACCEPT cancellation;
- concurrent ACCEPT;
- close/admission races;
- UDP/control receive close;
- shared add/remove/close races;
- retry cancellation;
- queue-full behavior.

### Resource/soak

- repeated STREAM;
- repeated datagram;
- repeated shared mixed child lifecycle;
- repeated failed/cancelled operations;
- bridge restart;
- zero retained ownership counters.

### CI/guards

- API guard self-test;
- protocol boundary positive control;
- schema validation;
- Linux stable/MSRV;
- macOS stable;
- Windows stable.

## 11. Required verification commands

At minimum:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
cargo +1.89 check --locked --workspace --all-targets
cargo +1.89 test --locked --workspace --all-targets
cargo +1.89 clippy --locked --workspace --all-targets --all-features -- -D warnings
python3 scripts/check-proto-boundary.py
python3 scripts/check-api-snapshot.py
```

Also execute each guard self-test/mutation mode and record exact hosted CI run identifiers
for the closure head.

## 12. Documentation updates

- README foundation status;
- async/blocking usage;
- retry terminology and guarantees;
- cancellation/close semantics;
- supported host platforms;
- public API migration note;
- API baseline;
- roadmap/registry;
- strict corrective closure record.

## 13. Acceptance criteria

M006 closes only when:

1. M005 has closed its interoperability/protocol corrective.
2. Public identity/trust-sensitive APIs have been reviewed and corrected before FFI.
3. Retry API naming matches actual behavior.
4. Blocking parity exists for every supported async capability or has an explicit justified
   exclusion.
5. Required cancellation/contention races are tested.
6. Mixed lifecycle soak shows no retained owned resources.
7. API guard detects synthetic API mutations.
8. Protocol boundary guard retains its positive control.
9. Linux stable and Rust 1.89 hosted lanes pass.
10. macOS stable passes.
11. Windows stable passes, or a concrete platform blocker is recorded and the platform is
    removed from the declared support set.
12. no live-router truth from M005 regresses.
13. M002–M004 conditional closures are reconciled additively into strict foundation
    authority.
14. only after this closure may M007 bindings or M008 service-tunnel integration become
    implementation-ready.

## 14. Stop conditions

Stop and register a successor if:

- M005 requires a material public ownership redesign;
- semantic API cleanup exposes incompatible identity models between routers;
- blocking parity requires a second protocol/session implementation;
- a cancellation race cannot be made ownership-safe without redesign;
- resource growth remains unexplained after deterministic ownership instrumentation;
- Windows/macOS require materially different protocol behavior rather than transport
  adaptation;
- API snapshot tooling cannot reliably observe the surface it claims to guard.

## 15. Closure evidence required

- exact M005 dependency closure;
- old/new API snapshot and migration summary;
- retry semantic decision;
- blocking parity matrix;
- cancellation/contention matrix;
- resource/soak counters;
- guard mutation results;
- hosted CI run URLs/IDs and exact SHA;
- MSRV/stable local results;
- residual platform limitations;
- explicit successor audit for M007/M008.

## 16. Handoff notes

Do not combine M007 bindings into this corrective. Bindings amplify every public API mistake
and should begin only after the post-live, post-race-tested Rust surface is frozen.
