# i2pr-sam

`i2pr-sam` is a clean-room Rust SAM client and tunnel-management toolkit for I2P.

The project is intentionally separate from the `dbowm91/i2pr` router. i2pr owns a SAM
**server** adapter over its router services; this repository owns the reusable SAM
**client** side: wire protocol/state, client sessions, router interoperability, language
bindings, and eventually a standalone tunnel manager/daemon.

Current status: **planning foundation only; implementation has not started**.

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

The exact pinned revisions used for implementation evidence are frozen by Milestone 001.

## License

No repository license has been selected yet. Do not copy source from reference
implementations. A license must be selected before public package distribution.
