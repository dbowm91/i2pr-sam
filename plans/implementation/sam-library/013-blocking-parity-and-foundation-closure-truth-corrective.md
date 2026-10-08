# 013 — Blocking parity and foundation closure-truth corrective

Class: corrective verification + invariant + polish.

Status: **closed**; see `plans/closure/sam-library/013-status.md`.

Repository baseline: `15f93089040691a1354d9c3b24d31a586ed655af`.

Corrects the current authority derived from:

- `plans/closure/sam-library/006-status.md`;
- `plans/closure/sam-library/004-status.md`;
- the blocking facade test surface under `crates/i2pr-sam-blocking/`.

## 1. Objective

Bring the blocking facade's evidence up to the level claimed by M006 and establish a
truthful final non-live foundation baseline before bindings are planned.

At the current head the blocking crate exposes wrappers for Destination/naming, STREAM,
ordinary datagrams, shared owners/children, timeouts, and close semantics. However the
repository has only one blocking integration-test file,
`crates/i2pr-sam-blocking/tests/stream_parity.rs`, containing two STREAM tests.

M006's closure nevertheless records:

> blocking facade covers datagram/shared/control-socket modes with parity — met

That statement is not supported by the checked-in test evidence.

M013 adds the missing parity tests, fixes any defects they expose, and adds an additive
closure correction without rewriting M006's historical record.

## 2. Why this milestone is ready

This is entirely deterministic and does not depend on live router qualification or M012.
It may execute in parallel with M012.

M011 remains the live wire-evidence gate. M013 is the blocking/API verification gate.

## 3. Current evidence

The blocking crate currently wraps at least:

- client connect/connect-with-policy;
- capabilities;
- session identity;
- peer resolution;
- Destination generation and naming lookup;
- STREAM session connect/accept/accept_with/close;
- ordinary datagram create/send/recv/close and transport/style/identity;
- shared owner create/identity/dialect/add/remove/close;
- shared STREAM connect/accept;
- shared datagram send/recv;
- timeout setters.

The only checked-in blocking integration tests prove:

1. STREAM CONNECT starts payload after the status line;
2. non-silent STREAM ACCEPT consumes the peer identity block before payload.

There is no checked-in blocking test for ordinary datagrams, control-socket datagrams,
D2/D3 refusal/fallback behavior, shared children, timeout cleanup, nested-runtime
rejection, naming/Destination parity, or unsupported capability propagation.

## 4. Invariants

- blocking remains a facade over the canonical async implementation;
- no blocking-only SAM parser or session state machine is added;
- every supported blocking operation preserves the async trust/identity types;
- timeout does not detach an async task that continues mutating state after the blocking
  call returns;
- DATAGRAM2/DATAGRAM3 never fall back to legacy v1/v2 control-socket semantics;
- shared children preserve the owner's concrete `SessionIdentity`;
- nested Tokio runtime use fails deterministically rather than deadlocking;
- parity means same semantic result/error, not merely same method name.

## 5. Scope

### In scope

- deterministic blocking parity fixture(s);
- ordinary UDP-forwarded DATAGRAM/RAW;
- legacy control-socket DATAGRAM/RAW;
- DATAGRAM2/3 supported-mode parity and forbidden-mode rejection;
- shared STREAM child;
- shared datagram child;
- owner/child identity;
- add/remove/close;
- naming/Destination generation;
- unsupported/error propagation;
- timeout behavior and cleanup;
- nested-runtime rejection;
- blocking resource-accounting assertions where observable;
- closure/registry/roadmap reconciliation.

### Out of scope

- live router qualification;
- C ABI/Python;
- service-tunnel adapter;
- a second blocking implementation;
- established-session auto-recovery.

## 6. Required production/test changes

### A. Shared deterministic bridge fixture

Prefer a reusable deterministic bridge fixture shared with the async tests or a small
blocking-specific wrapper around the same wire scripts.

Do not copy hundreds of lines of subtly divergent SAM mock behavior into every test file.

The fixture must be able to script:

- HELLO;
- DEST/NAMING;
- SESSION CREATE;
- STREAM CONNECT/ACCEPT;
- UDP forwarding;
- control-socket DATAGRAM/RAW bodies;
- shared SESSION ADD/REMOVE;
- delayed response for timeout tests;
- explicit `INVALID_STYLE` / transient failures.

### B. Ordinary datagram parity

Add blocking tests for:

- DATAGRAM1 UDP send/receive with authenticated source;
- RAW UDP send/receive with no source identity and correct HEADER behavior;
- DATAGRAM1 control-socket SEND/RECEIVED;
- RAW control-socket SEND/RECEIVED;
- D2/D3 supported data path;
- D2/D3 explicit refusal if caller selects legacy control-socket transport;
- FROM_PORT/TO_PORT/PROTOCOL parity;
- bounded receive timeout;
- queue/drop accounting where the async surface exposes it.

### C. Shared-session parity

Add blocking tests for:

- concrete owner `SessionIdentity`;
- child identity equals owner identity;
- STREAM child connect payload;
- STREAM child accept peer block;
- datagram child send/receive;
- duplicate/conflicting child rejection;
- remove child;
- sibling remains usable after removal;
- owner close invalidates child operations.

### D. Client utility parity

Add blocking coverage for:

- Destination generation returning typed values;
- naming lookup/typed peer resolution;
- capability result;
- unsupported style propagation;
- permanent versus transient errors where exposed.

### E. Runtime/timeout semantics

Add explicit tests for:

- invocation from inside a Tokio runtime -> `NestedRuntime`;
- blocking read timeout;
- blocking datagram receive timeout;
- timeout does not leave a later unexpected successful operation or retained resource;
- close remains idempotent.

If a blocking timeout reveals that the underlying future continues running after return,
fix ownership rather than adjusting the test.

### F. Closure-truth reconciliation

Do not edit M006's historical evidence to pretend these tests existed when it closed.

Add `plans/closure/sam-library/013-status.md` when complete and update current planning
to state:

- M006's implementation is preserved;
- M013 supplies the missing blocking parity evidence;
- strict foundation closure still additionally requires M011 live evidence.

## 7. Ordered work packages

### WP A — reusable blocking fixture

Exit: fixture can deterministically script all blocking capability families.

### WP B — utility + ordinary datagram parity

Exit: Destination/naming and all ordinary datagram transport/style semantics are covered.

### WP C — shared parity

Exit: shared STREAM/datagram child lifecycle and concrete identity are covered.

### WP D — timeout/runtime/error parity

Exit: nested runtime, timeouts, close, and unsupported/error propagation are proven.

### WP E — closure reconciliation

Exit: current planning no longer relies on unsupported M006 parity claims.

## 8. Failure, cancellation, restart, and contention semantics

- blocking timeout returns one terminal result for that operation;
- timed-out operations cannot later deliver data into a subsequent call;
- Drop/close releases the async resource owner;
- test servers must have bounded join/teardown and may not hang the suite after an
  assertion;
- shared owner close wakes/terminates child operations;
- no test relies on unbounded wall-clock sleeps.

## 9. Compatibility and migration

No public API change is expected. If parity tests expose a semantic defect, pre-1.0 fixes
are permitted and must update `docs/api-migration.md` if public behavior/signatures
change.

Bindings must target the post-M013 surface.

## 10. Required tests

At minimum one positive and relevant negative test for:

- generate Destination;
- lookup/resolve;
- STREAM connect;
- STREAM accept;
- nested-runtime rejection;
- STREAM timeout;
- DATAGRAM1 UDP;
- RAW UDP;
- DATAGRAM1 control socket;
- RAW control socket;
- D2;
- D3;
- D2/D3 legacy-control rejection;
- datagram timeout;
- shared owner identity;
- shared STREAM child;
- shared datagram child;
- child remove;
- sibling survival;
- owner teardown;
- unsupported capability propagation.

The closure should report blocking test counts separately from async/protocol counts.

## 11. Verification commands

At minimum:

```bash
cargo fmt --all -- --check
cargo test --locked -p i2pr-sam-blocking --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
python3 scripts/check-api-snapshot.py
python3 scripts/check-api-snapshot.py --self-test
python3 scripts/check-proto-boundary.py
python3 scripts/check-proto-boundary.py --self-test
```

Run the hosted CI matrix on the exact closure head.

## 12. Documentation updates

- blocking examples in `docs/client-usage.md`;
- M006 current-authority reconciliation;
- roadmap/registry;
- M013 closure;
- test inventory/counts.

Historical M004/M006 closures remain unchanged.

## 13. Acceptance criteria

M013 closes only when:

1. Blocking ordinary STREAM parity remains green.
2. Blocking Destination/naming parity exists.
3. Blocking DATAGRAM1 and RAW UDP paths have payload tests.
4. Blocking DATAGRAM1 and RAW control-socket paths have payload tests.
5. Blocking D2/D3 behavior matches async semantics, including legacy-control rejection.
6. Blocking shared STREAM payload is tested.
7. Blocking shared datagram payload is tested.
8. Shared owner/child concrete identity equality is tested.
9. Child remove/sibling/owner teardown semantics are tested.
10. Nested-runtime rejection is explicitly tested.
11. STREAM and datagram receive timeout cleanup are tested.
12. Unsupported/error propagation is tested.
13. No blocking-only protocol implementation is introduced.
14. Exact-head Linux/MSRV/macOS/Windows CI remains green.
15. Current planning states that only M011 live evidence remains after both M012 and M013
    close.

## 14. Stop conditions

Stop and register a successor if:

- blocking timeout cannot cancel/release the underlying async operation;
- full parity requires duplicating the protocol/session implementation;
- a supported async capability fundamentally cannot be represented synchronously without
  unsafe lifecycle behavior;
- parity tests expose a public semantic flaw that requires a broader async API redesign.

## 15. Closure evidence required

- blocking test inventory and count;
- parity matrix by capability;
- any defects found/fixed;
- timeout/resource results;
- nested-runtime result;
- API snapshot diff if any;
- exact-head hosted CI run IDs;
- additive reconciliation of M006 current authority.

## 16. Handoff notes

M013 does not replace M011. After M012 and M013 close, M011 should be the only remaining
foundation gate, and it should then be genuinely environmental/live rather than carrying
known repository defects or unsupported verification claims.
