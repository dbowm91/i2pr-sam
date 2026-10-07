# i2pr-sam Long-Term Specification

Status: canonical.

## 1. Product goal

Provide a maintainable, clean-room Rust implementation of the client side of I2P SAM v3
that application developers can use directly and that higher-level tunnel-management
software can compose without reimplementing protocol or privacy policy.

The library must be useful against multiple router implementations, including Java I2P,
i2pd, and i2pr, and must make router quirks explicit rather than leaking them throughout
application code.

## 2. Required capability families

The end state includes:

- SAM HELLO/version negotiation and explicit feature/capability representation;
- Destination generation/import/export and naming lookup;
- STREAM connect, accept, listen/forward semantics where safely applicable;
- legacy authenticated DATAGRAM;
- RAW datagrams;
- DATAGRAM2;
- DATAGRAM3 with explicit unauthenticated-source typing;
- SAM 3.3 shared-Destination sessions and subsessions;
- PRIMARY/MASTER compatibility policy based on observed router behavior;
- I2P ports/protocol metadata;
- bounded parser/serializer/state machines;
- cancellation, close, retry, timeout, and reconnect semantics;
- async Rust API plus blocking facade;
- stable C ABI and Python bindings after the Rust API is proven;
- an adapter from SAM sessions to the public `i2pr-service-tunnels` policy/filter core;
- a headless tunnel manager/daemon suitable for app sidecars and local administration;
- optional management/UI clients over that daemon, not policy duplicated in a WebUI.

## 3. Architectural invariants

1. Wire parsing and protocol state are separate from socket/runtime ownership.
2. No SAM-version number is used as the sole proof that an optional capability exists.
3. Router-specific differences are represented in one compatibility/capability layer.
4. Application-facing APIs do not expose raw router quirks when a stable semantic type
   can represent them.
5. STREAM state transitions are explicit: after successful CONNECT/ACCEPT, the control
   connection becomes a byte stream and command parsing stops on that connection.
6. All peer/router input is bounded before allocation or buffering.
7. Queues, pending operations, sessions, children, datagrams, and retries have named
   ceilings.
8. Cancellation and close release resources exactly once.
9. DATAGRAM3 source identity is never presented as authenticated.
10. RAW datagrams never synthesize peer identity.
11. Shared-Destination APIs preserve one Destination/linkability domain across the child
    sessions that intentionally share it.
12. Existing implementations are behavioral/reference oracles only; source copying is
    prohibited unless an explicit provenance decision says otherwise.
13. Foreign-language bindings wrap the canonical Rust semantics; they do not become a
    second protocol implementation.
14. The tunnel daemon owns process/runtime/config lifecycle; the reusable policy core
    consumed from i2pr remains transport-neutral.
15. Protocol support is advertised only after independent interoperability evidence.

## 4. Runtime ownership

The protocol crate is runtime-neutral.

The first production async client may use Tokio internally, but protocol/domain types
must not depend on task handles, global runtimes, or daemon process state. Runtime
portability is an implementation choice, not permission to make the protocol layer
runtime-aware.

The blocking facade owns its blocking strategy and must not require callers to understand
an async runtime.

## 5. Public API principles

Prefer semantic objects over stringly command assembly:

- `SamClient`
- `Destination` / `PrivateDestination`
- `Resolver`
- `StreamSession`, `StreamListener`, `SamStream`
- datagram session types with authenticated/unauthenticated source distinctions
- `SharedSession` plus child session handles
- `SamCapabilities` / router compatibility profile
- typed protocol and transport errors

The exact names are not frozen until implementation evidence, but these semantic roles
are.

## 6. Interoperability posture

The library targets the published SAM protocol while accepting narrowly justified
deployed compatibility differences. Compatibility work must record the router/version
and exact behavior observed.

The project must not call a router interoperable merely because HELLO succeeds.

## 7. Security and privacy

- No private Destination/key material in default logs, errors, metrics, or panic text.
- Credentials, private Destinations, and other secrets must have redacted `Debug`.
- Remote SAM bridges are not assumed trustworthy merely because a TCP connection
  succeeded.
- Authentication/TLS configuration is explicit when supported.
- Datagram source authenticity distinctions are preserved end-to-end.
- Tunnel-manager defaults must not unexpectedly expose local listeners to non-loopback
  interfaces.
- Application policy/filtering from `i2pr-service-tunnels` is applied before bytes are
  forwarded where the selected service profile requires it.

## 8. Distribution

Rust packages, C ABI, Python wheels, binaries, and daemon packaging are separate release
surfaces. Each requires its own compatibility and ownership evidence. No package
publication occurs until this repository has an explicit license and the relevant public
API is reviewed.
