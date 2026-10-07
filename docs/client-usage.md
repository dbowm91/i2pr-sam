# SAM client usage and boundaries

The async crate owns bridge connections and session control sockets. Each stream
connection performs a fresh HELLO and STREAM CONNECT, then retains the buffered reader
after the peer-identity block so neither identity metadata nor early peer bytes are lost. A
successful session owns its control socket until `close` or drop. Closing a session rejects
new operations; already returned stream handles remain owned by the caller.

```rust,no_run
use i2pr_sam::{ClientConfig, SamClient, SessionDestination};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = SamClient::connect(ClientConfig::new("127.0.0.1:7656".parse()?)).await?;
let session = client
    .create_stream_session(&SessionDestination::Transient, "app", &[])
    .await?;
let stream = session.connect("example.i2p", None, None).await?;
# let _ = stream;
# Ok(())
# }
```

## Session identity is a concrete Destination

`SessionDestination` says what a session should adopt, and the returned session identity is
a real `Destination` plus its canonical SHA-256 hash:

| Request | Meaning |
|---|---|
| `Transient` | the router generates a Destination and the client resolves it |
| `Generated` | the client generates a keypair on the utility connection and imports it |
| `Imported(Destination)` | import a public Destination the caller already controls |
| `WithKey { public, secret }` | import public text plus private material (offline-signed) |

A shared session **fails** with `SamError::IdentityUnavailable` when the router does not
reveal a concrete Destination. Returning the request token `TRANSIENT` is never treated as
identity, and every subsession reports the owner's identity, so a caller can assert that a
linkability domain really is one Destination:

```rust,no_run
# async fn example(client: &i2pr_sam::SamClient) -> Result<(), Box<dyn std::error::Error>> {
use i2pr_sam::{SessionDestination, SessionStyle, SharedDialect};

let shared = client
    .create_shared_session(&SessionDestination::Transient, "svc", SharedDialect::Primary, &[])
    .await?;
let datagram_child = shared.add_child("svc-data", SessionStyle::Datagram, &[]).await?;
assert_eq!(datagram_child.identity(), shared.identity());
println!("linkability domain {}", shared.identity().hash());
# Ok(())
# }
```

Private Destination text never appears in `Debug`, logs, or conformance artifacts; artifacts
carry the hash and the public Destination only.

## Datagram transports are explicit

Ordinary DATAGRAM1 and RAW sessions support two transports, and the choice is part of the
API rather than an implementation detail:

- `DatagramTransport::UdpForward` (default) - send to the SAM datagram port, receive through
  a forwarded UDP socket.
- `DatagramTransport::ControlSocketV1` - `DATAGRAM SEND`/`RAW SEND` on the session socket
  with unsolicited `DATAGRAM RECEIVED`/`RAW RECEIVED` lines carrying `SIZE` raw bytes.

DATAGRAM2 and DATAGRAM3 are refused for the control-socket transport before any command is
sent, because the specification excludes the v1/v2 mechanism for those styles. Shared
subsessions never expose the control-socket datagram path at all.

Forwarded RAW metadata is conditional: with `HEADER=true` (the default from SAM 3.2) the
bridge prepends FROM_PORT/TO_PORT/PROTOCOL; without it the datagram is bare payload and the
client reports the session's configured protocol rather than inventing wire metadata.

## Stream accepts announce the peer

A non-silent `STREAM ACCEPT` is followed by the router's peer identity block
(`$destination`, optional `FROM_PORT`/`TO_PORT`, blank line) and then payload. The client
consumes that block and exposes it as `SamStream::peer()`; the first bytes read from the
stream are always application payload. Use `accept_with(true)` for `SILENT=true` accepts,
where no block is sent.

## Capability is learned, not assumed

`SamCapabilities` starts at `Support::Unknown` and only moves to `Supported` after an
operation actually succeeds. `RESULT=INVALID_STYLE` records `Unsupported`; unreachable peers,
expired leasesets, and timeouts stay transient and never mark a capability unsupported.

## Blocking facade

The blocking crate runs the same async client on a private multithread Tokio runtime. It
returns `BlockingError::NestedRuntime` when used from inside another Tokio runtime, and
blocking stream reads and writes have explicit configurable deadlines. It mirrors the async
surface: typed ports, `DatagramTransport`, `accept_with`, peer identity, shared identity, and
`connect_with_policy`.

## Bounded protocol behavior

The protocol line limit is 16 KiB, the token count limit is 128, keys are limited to 128
bytes, and values and Destination/private-key text are limited to 8 KiB. A Destination token
used in a command is additionally capped at a quarter of that so one framed command cannot
exhaust the line budget. Control-socket datagram deliveries land in a queue bounded by both
message count and bytes; overflow drops the newest delivery and increments a counter that
`dropped_datagrams()` and `resource_usage()` expose. Router-controlled input never drives
unbounded growth, and no peer-controlled panic path exists.

## Resource observability

`i2pr_sam::resource_usage()` reports live sessions, live streams, peak concurrency, open
control sockets, and dropped datagrams. Counters are observability aids, never enforcement:
they let a soak test assert that closes actually release resources.

## Conformance

`sam-conformance` performs semantic operations and records what it observed. A row is
`pass` only when payload crossed the link and both ends agreed on the exact bytes; a session
that merely came into existence is `create_only`; an unimplemented router feature is
`unsupported`; a lane that could not run names its blocking diagnostic. Artifacts validate
against `specs/conformance.schema.json` via `scripts/check-conformance-artifact.py`, which
rejects an artifact that claims a pass without payload evidence. Live rows require
`--peer-endpoint`: one bridge can prove structure, but only two can prove an exchange.