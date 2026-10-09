# 018 — Injected SAM Connection Provider and Socket-Neutral Client Transport

Class: infrastructure + invariant + public API extension.

Status: **blocked on M017 closure/post-merge qualification**.

Repository baseline: `28ee0796cc1e4cdcee3104013a652c1b8c56a1ff` on
`plans/017-foundation-closure-reconciliation`.

Primary downstream consumer:

- `dbowm91/i2pr-mail` M013 managed-app SAM adoption, where each SAM connection must be
  supplied by the router's managed-app logical `sam` service rather than by
  `TcpStream::connect`.

Related downstream router work:

- `dbowm91/i2pr` SAM/368 adds the server-side port-aware SAM 3.3 profile;
- `dbowm91/i2pr` Managed native app runtime/385 adds the first qualified Linux
  `Secured` application sandbox.

## 1. Objective

Make the canonical `i2pr-sam` client usable over an injected reliable byte-stream
provider without weakening or duplicating its existing TCP SAM implementation.

Today the protocol/session model is already correct:

- a session retains its SESSION CREATE control connection;
- every STREAM CONNECT opens a fresh connection and performs its own HELLO;
- typed `Port` values are emitted as `FROM_PORT`/`TO_PORT`;
- STREAM, naming, shared-session and control-socket datagram logic live in the canonical
  async client;
- the blocking facade delegates to that client.

The remaining coupling is concrete transport: `SamClient`, `StreamSession`,
`SharedSession`, control-socket datagram machinery, and `SamStream` ultimately own
`tokio::net::TcpStream`, while `open_control` always calls
`TcpStream::connect(config.endpoint)`.

M018 introduces one bounded, object-safe **connection provider** abstraction so callers
may supply each reliable SAM connection. The default provider remains the current TCP
behavior. An injected provider receives no protocol strings and owns no SAM state; it
only returns a fresh `AsyncRead + AsyncWrite` byte stream or a typed transport-open
failure.

This is the reusable seam the dedicated SAM library was created to own. Downstream
projects must not fork the SAM state machine merely because their transport to the SAM
server is not a host TCP socket.

## 2. Why this milestone is blocked / unblock condition

The technical prerequisites are satisfied by M011–M016. PR #2 has now merged the
reconciled foundation to `main` at `46d06ea20ed0d4653e2af5ad9b4c017789a28342`,
but M017 remains in its closing phase: post-merge main CI run `37950049744` was still
queued at the latest audit, the M017 closure record has not landed, and branch/registry
cleanup is not yet complete.

M018 therefore remains **blocked** until M017 closes. The merge itself is necessary but
not sufficient under this repository's closure rule.

M018 becomes **ready** when M017 closes and:

- post-merge exact-head hosted CI is green;
- the M017 closure record is present on `main`;
- the registry/roadmap identify `main` as strict foundation authority;
- stale foundation branch/PR authority is retired or explicitly reconciled.

At implementation start, rebase/recreate this plan branch from the closed M017 mainline
rather than implementing against this intentionally pre-closure planning branch.

## 3. Current implementation evidence

Current foundation code already exposes the right protocol behavior but hard-codes the
I/O type:

- `SamClient` owns `utility: Mutex<Control<TcpStream>>`;
- `resolve_identity` accepts `Control<TcpStream>`;
- ordinary and shared sessions retain `Control<TcpStream>`;
- `SamStream` wraps `Prefixed<BufReader<TcpStream>>`;
- `ControlDatagramLink::new` accepts a `TcpStream`;
- `open_control` wraps `TcpStream::connect(config.endpoint)` in
  `connect_timeout`.

The lower `Control<S>` and `Prefixed<R>` helpers are already generic over
`AsyncRead + AsyncWrite + Unpin`, so the protocol engine does not intrinsically need a
TCP stream. The concrete type has simply propagated through the higher-level structs.

The client already supports typed STREAM ports:

```text
session.connect(destination, Option<Port>, Option<Port>)
 -> STREAM CONNECT ... FROM_PORT=<n> TO_PORT=<n>
```

and `docs/client-usage.md` already documents that every stream connection performs a
fresh HELLO + STREAM CONNECT while the successful session retains its control socket.
M018 must preserve that behavior exactly.

## 4. Invariants

- One protocol implementation remains authoritative. The provider seam may not fork
  command/reply/session state.
- Default `SamClient::connect(ClientConfig)` TCP behavior remains source-compatible
  unless an unavoidable API issue is explicitly recorded.
- Injected mode opens **one fresh provider stream for every SAM connection** the existing
  client would have opened as TCP.
- Provider code never receives private Destination material except what the normal SAM
  bytes themselves carry after the connection is returned.
- `connect_timeout` remains enforced by `i2pr-sam`; provider implementations do not
  gain an independent hidden retry clock.
- Existing bounded retry/admission accounting remains the sole initial-connect retry
  owner.
- Provider failures are typed and cannot be mistaken for a SAM protocol rejection.
- No provider may return the same concurrently-owned connection object twice.
- No unbounded queues or detached tasks are introduced by the abstraction.
- TCP/UDP host networking remains isolated to the default network provider paths.
- Injected mode must not silently fall back to TCP or UDP forwarding.
- Existing DATAGRAM3 trust typing, shared-session identity, teardown, and resource
  accounting remain unchanged.
- No local `unsafe` is introduced.
- MSRV remains 1.89.

## 5. Scope

### In scope

- an object-safe reliable SAM connection-provider API;
- an erased async duplex stream type;
- default TCP provider preserving current endpoint behavior;
- provider-backed utility/control/session/STREAM connections;
- provider-backed control-socket DATAGRAM/RAW where they use the same reliable SAM
  connection semantics;
- deterministic tests proving exact provider-open counts and connection roles;
- API snapshot/migration documentation;
- blocking-facade disposition for the new provider API;
- a no-host-socket injected-mode guard.

### Explicitly out of scope

- a managed-app protocol implementation;
- i2pr-specific capability framing;
- changing SAM wire syntax;
- implementing SAM server behavior;
- UDP-forwarding over an injected provider;
- inventing a virtual UDP API;
- changing DATAGRAM2/3 semantics;
- service-tunnel policy;
- C/Python bindings;
- daemon/UI work;
- live i2pr qualification;
- publishing crates or selecting a repository license.

## 6. Required production changes

### A — socket-neutral duplex type

Introduce one public or deliberately semi-public erased stream abstraction, for example:

```rust
pub trait SamDuplex: AsyncRead + AsyncWrite + Unpin + Send {}
pub type BoxSamDuplex = Box<dyn SamDuplex>;
```

or a reviewed equivalent.

It must be usable by `BufReader`, `Prefixed`, split/read/write paths, and the
control-socket datagram link without unsafe pinning tricks.

### B — object-safe connection provider

Use an object-safe provider shape with no new async-trait dependency required merely for
ergonomics. A boxed future is acceptable:

```rust
pub trait SamConnectionProvider: Send + Sync {
    fn open(&self) -> Pin<Box<
        dyn Future<Output = Result<BoxSamDuplex, TransportOpenError>> + Send + '_
    >>;
}
```

The exact public names may differ.

`TransportOpenError` should preserve only generic transport categories useful to the
SAM client, such as:

- unavailable;
- permission denied;
- resource limit;
- connection refused;
- I/O;
- cancelled where the provider can observe caller cancellation.

Do not add i2pr-specific capability or app identifiers to this crate.

### C — separate common runtime policy from TCP endpoint configuration

Do not require injected callers to invent a fake `SocketAddr`.

Preserve `ClientConfig::new(SocketAddr)` for current TCP callers, but factor the common
version/time/resource/retry settings into a socket-neutral configuration value consumed
by both paths.

Acceptable shapes include:

- `ClientRuntimeConfig` + existing `ClientConfig` TCP wrapper; or
- a transport enum/builder that makes the active provider explicit.

Unacceptable shape: an injected API that accepts `ClientConfig` and silently ignores
its endpoint/datagram fields.

### D — default TCP provider

Move the existing `TcpStream::connect` call into one provider implementation.

Static review should be able to show that a provider-backed `SamClient` has no second
TCP path. The old convenience constructor delegates to this provider.

### E — erase concrete TcpStream from reliable protocol ownership

Refactor reliable-connection owners to the erased stream:

- utility control;
- identity lookup;
- ordinary stream session control;
- shared session control;
- STREAM CONNECT/ACCEPT data connections;
- control-socket DATAGRAM/RAW links.

Keep UDP-forwarding types concrete and network-specific. If an injected provider caller
requests a mode that necessarily needs local UDP forwarding, fail with a typed
unsupported-transport error **before** binding any UDP socket.

### F — provider-aware constructors

Add an explicit constructor such as:

```text
SamClient::connect_with_provider(runtime_config, Arc<dyn SamConnectionProvider>)
```

The constructor performs HELLO on a provider connection exactly as the TCP path does.

Sessions clone/retain the provider handle so each later STREAM CONNECT can request a new
connection.

### G — blocking facade disposition

The blocking facade must not grow a second SAM implementation.

Either:

1. expose `BlockingClient::connect_with_provider` using the same provider inside its
   owned Tokio runtime; or
2. explicitly document that injected providers are async-only in M018 and register
   blocking-provider parity as a named follow-up **before** calling the new provider API
   generally stable.

Preferred outcome is parity in this milestone if it does not require a second executor
or provider contract.

## 7. Ordered work packages

### WP1 — API/ownership freeze

Re-freeze the post-M017 public surface and write the connection-provider design in
`docs/client-usage.md` / API migration documentation.

Acceptance: one type/ownership diagram and exact default-vs-injected constructor
semantics.

### WP2 — provider substrate

Land the erased duplex type, provider trait, generic transport-open error and default TCP
provider.

Acceptance: synthetic provider opens one byte stream and completes HELLO without any
host socket.

### WP3 — reliable connection refactor

Replace reliable `TcpStream` fields with the socket-neutral type through utility,
ordinary STREAM, shared STREAM, and control-socket datagram paths.

Acceptance: existing deterministic suite passes with the default provider and equivalent
provider-backed fake streams.

### WP4 — injected lifecycle qualification

Add a provider fixture that counts and labels opens.

Required evidence:

- SamClient connect opens utility connection;
- stream SESSION CREATE opens/retains its own control connection;
- each STREAM CONNECT opens a distinct provider connection;
- session close releases the control connection;
- stream close releases only that data connection;
- control loss invalidates the session as before;
- shared children preserve owner semantics.

### WP5 — network-authority negatives

Prove injected mode cannot call the TCP provider or bind UDP forwarding accidentally.

If a UDP-forwarding-only API is invoked under injected mode, refuse before socket
allocation.

### WP6 — API snapshot/docs/closure

Reconcile public API snapshot, migration docs, README usage examples, registry/roadmap,
and closure record.

## 8. Failure, cancellation, restart, and contention semantics

- Provider open is bounded by the client's existing `connect_timeout`.
- Initial retry policy may reopen through the provider exactly as it currently retries
  TCP, using the same max attempts/backoff/elapsed/admission ceiling.
- A permanent/permission provider failure is not retryable.
- A transient/unavailable provider failure may follow the existing bounded retry
  classification only if explicitly mapped.
- Failure opening one STREAM data connection does not close the retained session-control
  connection.
- Loss of the retained session-control connection keeps the existing session-invalid
  behavior.
- Dropping a provider-backed `SamStream` releases the same operation permit/resource
  counters as TCP.
- Cancellation while a provider future is pending must release its admission/resource
  ownership.
- No provider call may remain detached after client/session drop.
- Provider implementations own their external channel lifetime; i2pr-sam owns only the
  fresh connection object returned for one SAM connection.

## 9. Compatibility and migration

M018 is additive for existing TCP users.

`SamClient::connect(ClientConfig)`, ordinary session APIs, typed ports, and blocking
TCP usage should remain source-compatible.

The public API snapshot must record:

- new provider/runtime-config types;
- any necessary generic-erasure changes;
- whether `SamStream` remains the same concrete public type.

Do not expose implementation generics throughout `StreamSession`/ `SharedSession`;
that would turn one transport extension into pervasive downstream type churn.

No SAM wire migration.

## 10. Required tests

At minimum:

- default TCP constructor regression;
- injected utility HELLO exact bytes;
- provider open max+1/retry/admission bounds;
- session-control retention;
- two STREAM CONNECTs produce two distinct provider opens after SESSION CREATE;
- FROM_PORT/TO_PORT exact-byte preservation;
- CONNECT failure leaves owner session valid;
- owner loss invalidates streams/children;
- naming and Destination generation over provider;
- control-socket DATAGRAM/RAW over provider;
- UDP-forward mode refused in injected mode before bind;
- shared PRIMARY/MASTER STREAM child through provider;
- provider denied/unavailable/resource-limit/I/O error mapping;
- connect timeout/cancellation cleanup;
- no leaked resource counters after repeated provider-backed open/close;
- default TCP and injected provider produce identical protocol transcripts for the same
  deterministic scenario;
- API snapshot and boundary-guard mutation tests.

## 11. Required verification commands

After M017 closes, run the full repository floor at the rebased implementation head:

```text
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
git diff --check
```

Add focused provider tests under `i2pr-sam` and `i2pr-sam-blocking` as applicable.

Hosted Linux/MSRV/macOS/Windows CI must be green at the exact closure head. Live-router
interop is not required because the default protocol/wire behavior does not change, but
any live regression run actually performed should be recorded.

## 12. Documentation updates

- `docs/client-usage.md` — injected provider ownership and limitations;
- `docs/api-migration.md` — additive provider API;
- `README.md` — transport-neutral client capability without implying i2pr integration;
- `api/public-types.txt`;
- `plans/subsystems/sam-library-roadmap.md`;
- `plans/registry.md`;
- `plans/closure/sam-library/018-status.md`.

Do not claim i2pr managed-app compatibility in this repo until a downstream integration
actually qualifies it.

## 13. Acceptance criteria

M018 closes only when:

- the canonical async client can operate with no `TcpStream` in its reliable SAM
  connection path;
- the default TCP constructor remains compatible and green;
- injected callers supply every utility/control/data SAM connection through one bounded
  provider abstraction;
- ordinary STREAM and shared STREAM preserve exact lifecycle semantics;
- typed FROM_PORT/TO_PORT behavior is unchanged;
- control-socket DATAGRAM/RAW work or have an explicitly narrower evidence-backed
  disposition;
- host UDP-forwarding APIs fail before bind in injected mode;
- no SAM protocol logic is duplicated;
- resource/retry/cancellation accounting remains bounded;
- public API snapshot is reconciled;
- exact-head cross-platform/MSRV CI is green;
- no high/medium finding remains open.

## 14. Stop conditions

Stop and register a corrective if:

- the abstraction requires pervasive public generics over every session type;
- injected mode can accidentally reach the default TCP/UDP path;
- a single provider stream would have to be reused for multiple concurrent SAM
  connections;
- provider cancellation cannot release resources under the existing ownership model;
- blocking parity would require a second SAM implementation;
- the refactor changes SAM wire behavior beyond a separately justified bug fix;
- implementation starts before M017 establishes `main` as the foundation authority.

## 15. Closure evidence required

Record:

- post-M017-closure baseline and implementation SHA;
- public API before/after summary;
- provider ownership/lifetime diagram;
- exact connection-open count matrix;
- TCP vs injected transcript equivalence;
- typed provider failure/retry matrix;
- STREAM/session/shared teardown evidence;
- UDP-forward refusal evidence;
- resource/cancellation soak results;
- API/protocol guard mutation evidence;
- local stable/MSRV floor;
- exact-head hosted CI;
- unresolved findings and successor audit.

## 16. Successor decision

On M018 closure:

- downstream applications may implement transport adapters that return fresh reliable
  SAM byte streams without copying SAM protocol/session code;
- `dbowm91/i2pr-mail` M013 may consume the provider API once it also has its
  managed-app v1 logical-stream client;
- M007 bindings remain independently gated by their own planning/licensing;
- M008 service-tunnel adapter remains a separate policy integration and is not replaced
  by this generic provider seam.
