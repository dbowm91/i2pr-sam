# i2pr-sam

`i2pr-sam` is a clean-room Rust SAM client and tunnel-management toolkit for I2P.

The project is intentionally separate from the `dbowm91/i2pr` router. i2pr owns a SAM
**server** adapter over its router services; this repository owns the reusable SAM
**client** side: wire protocol/state, client sessions, router interoperability, language
bindings, and eventually a standalone tunnel manager/daemon.

Current status: **pre-1.0 implementation foundation, conditionally closed**. The protocol
codec, async client, blocking facade, both datagram transports, concrete shared-session
identity, the conformance runner, the interop harness, and hosted CI are implemented.

The one thing that does **not** exist is live router evidence: no Java I2P, i2pd, or i2pr
payload row has ever passed. The implementation host has no provisioned router or reachable
SAM bridge. UDP probe silence at arbitrary endpoints is inconclusive and is no longer used
to infer an egress policy. Milestones 005
and 006 are therefore conditionally closed, and milestone 011 carries the live payload
matrix as the gate for strict closure. See `plans/closure/sam-library/` for exact
boundaries.

## Target shape

The intended architecture is layered:

- a runtime-neutral SAM protocol/state-machine crate;
- a canonical async Rust client;
- a blocking Rust facade;
- C ABI and Python bindings;
- a service-tunnel adapter consuming the public `i2pr-service-tunnels` policy core;
- a standalone tunnel manager/daemon with configuration and management surfaces.

The first implementation line is deliberately narrower than that end state. See
[`plans/registry.md`](plans/registry.md) and
[`plans/subsystems/sam-library-roadmap.md`](plans/subsystems/sam-library-roadmap.md).

## Protocol scope

The long-term target is current SAM v3 behavior, including STREAM, legacy DATAGRAM,
RAW, DATAGRAM2, DATAGRAM3, and shared-Destination PRIMARY/MASTER session semantics.
Version negotiation alone is not treated as a capability guarantee; deployed routers
differ in which 3.x features they implement.

## Clean-room policy

Protocol behavior is derived from published I2P specifications and independently
observed interoperability. Existing SAM libraries and router implementations may be
used as behavioral/reference oracles, not copied as source.

The initial reference set includes:

- official SAM v3 documentation: <https://geti2p.net/en/docs/api/samv3>
- Java I2P;
- i2pd;
- I2P+ where useful for deployed-compatibility checks;
- i2pr;
- `go-i2p/go-sam-go`, `yosemite`, `go-i2p/sam3`, and `sam-forwarder` as
  API/behavior references only.

The exact pinned revisions used for implementation evidence are frozen by Milestone 001
and re-verified by Milestone 005; see
[`specs/references/sam-v3-reference-freeze.md`](specs/references/sam-v3-reference-freeze.md).

## Workspace crates

- `i2pr-sam-proto` contains bounded runtime-neutral SAM syntax, typed protocol values,
  state legality, capabilities, and distinct authenticated/unverified/raw datagram types.
- `i2pr-sam` is the canonical Tokio async client for HELLO, Destination generation,
  naming, STREAM, both datagram transports (UDP forwarding and the v1/v2-compatible
  control socket), and shared-owner child lifecycle with concrete Destination identity.
- `i2pr-sam-blocking` is a synchronous facade over the async crate. Calls made from an
  existing Tokio runtime return a typed `NestedRuntime` error.

The implementation is pre-1.0 and not published. Use the async or blocking crate for
application code; see [`docs/client-usage.md`](docs/client-usage.md) for examples and
lifecycle limits and [`docs/api-migration.md`](docs/api-migration.md) for the API changes
made by the two most recent milestones.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace
python3 scripts/check-proto-boundary.py --self-test
python3 scripts/check-api-snapshot.py --self-test
python3 scripts/check-conformance-artifact.py --self-test
```

Hosted CI lanes run on Linux stable, Linux MSRV 1.89, macOS, and Windows. Live router
qualification runs locally or on an operator-provisioned host:

```bash
python3 scripts/interop/udp_egress_probe.py
python3 scripts/interop/qualify.py --all --peer-endpoint 127.0.0.1:7657
```

The UDP probe is advisory: a reply proves one request/reply path works, while silence is
`unknown`. Qualification proceeds to the configured SAM bridge regardless. There is no
GitHub live-interoperability workflow because this repository has no registered self-hosted
Actions runner. Run locally or on an operator-provisioned host that can reach both bridge
endpoints; pass `--peer-endpoint HOST:PORT` for payload exchange.

## License

No repository license has been selected yet. Do not copy source from reference
implementations. A license must be selected before public package distribution.
