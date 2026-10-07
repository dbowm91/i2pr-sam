# SAM Library M003 closure

Status: **conditionally closed**.

Implementation SHA: `956238fcced742836833befaf6e748e97f435001`.

## Evidence

- D1/D2 authenticated sources, D3 unverified 32-byte source hashes, and RAW no-source
  messages are distinct Rust types. D3 cannot convert to `Destination` through the public
  API.
- Ordinary datagram sessions use bounded SAM UDP frames and loopback forwarding by
  default. `DatagramForwardConfig` permits an explicitly configured interface, advertised
  host, and fixed port for remote bridge forwarding. Payload size is capped by configuration
  (32 KiB default).
- Receive parsing preserves FROM_PORT/TO_PORT and RAW PROTOCOL metadata. Close wakes a
  pending receive. Shared sessions retain one owner control connection, one Destination,
  bounded children, MASTER/PRIMARY dialect selection, transactional child registration,
  listener tuple conflict checks, add/remove, and owner/child invalidation.
- Mock evidence covers D1 UDP send/receive, D3 source trust decoding, RAW no-source
  decoding, shared child add/remove, same-Destination ownership, duplicate listener tuple
  rejection, and owner teardown.

## Commands executed

The stable and Rust 1.89 workspace check, test, and Clippy commands listed in M001 closure
passed. Formatting, warning-free documentation, dependency boundary, and API snapshot
checks passed. The final stable workspace test run reported 18 passed.

## Router matrix

Java I2P, i2pd, and i2pr rows for DATAGRAM, RAW, DATAGRAM2, DATAGRAM3, shared MASTER, and
shared PRIMARY are `not_run`; see `specs/conformance-observations.csv` and
`specs/live-router-qualification.md`. None of these styles is advertised as supported from
HELLO version alone. The current SAM documentation says no DATAGRAM2/3 implementation is
known; this is treated as an interoperability risk.

## Deviations and residual risk

The implementation uses UDP forwarding for datagram receives; the SAM control-socket
receive mode and legacy control-socket DATAGRAM SEND/RAW SEND are not implemented. Shared
child payload operations exist for STREAM and UDP datagrams, but no live same-Destination
exchange was possible. The caller chooses MASTER or PRIMARY explicitly; there is no Auto
fallback because no safe side-effect-free fallback evidence was available. Remote
forwarding requires network reachability and external firewall/NAT setup.

## Successor audit

M004 was conditionally unblocked: no semantic divergence was observed in deterministic
tests, and the runtime facade, bounded initial-connection retry policy, lifecycle harness,
conformance runner, and API inventory could be completed independently. Router matrix
qualification remains open.
