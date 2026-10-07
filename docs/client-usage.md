# SAM client usage and boundaries

The async crate owns bridge connections and session control sockets. Each stream
connection performs a fresh HELLO and STREAM CONNECT, then retains the buffered reader
after the status line so early peer bytes are not lost. A successful session owns its
control socket until `close` or drop. Closing a session rejects new operations; already
returned stream handles remain owned by the caller.

```rust,no_run
use i2pr_sam::{ClientConfig, SamClient};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = SamClient::connect(ClientConfig::new("127.0.0.1:7656".parse()?)).await?;
let session = client.create_stream_session("TRANSIENT", "app", &[]).await?;
let stream = session.connect("example.i2p", None, None).await?;
# let _ = stream;
# Ok(())
# }
```

Set `SIGNATURE_TYPE=7` for transient identities; the current API applies this default.
Persistent private Destination text should be stored by the application as secret
material. `SecretDestination` redacts its `Debug` representation.

The blocking crate runs the same async client on a private multithread Tokio runtime.
It returns `BlockingError::NestedRuntime` when used from inside another Tokio runtime.
Blocking stream reads and writes have explicit configurable deadlines.

## Bounded protocol behavior

The protocol line limit is 16 KiB, the token count limit is 128, keys are limited to 128
bytes, and values and Destination/private-key text are limited to 8 KiB. These conservative
ceilings exceed current SAM Destination/private-key examples while preventing router
controlled input from driving unbounded growth. SAM does not specify a maximum line size;
the local limit is therefore an implementation policy and may reject unusually large
router extension fields.

## Datagram trust and transport

SAM UDP forwarding defaults to loopback. The client binds its forwarding socket to
`127.0.0.1` and the bridge address/UDP port is configurable. For a remote bridge, configure
`ClientConfig::datagram_forward` with a local bind interface, a host address reachable from
the bridge, and a fixed forwarded port. The bridge must be able to route to that address;
the library does not create firewall or NAT rules. D1 and D2 sources are exposed
as authenticated Destination text, D3 sources as a 32-byte unverified hash, and RAW
messages carry no source identity. D3 hashes are never promoted to Destinations. D1/RAW
payloads are bounded to the configured limit (32 KiB by default); the recommended delivery
size is smaller because I2P message reliability falls as datagrams grow.

`SharedSession` keeps one control connection and one destination for its child sessions.
The caller selects the `MASTER` or `PRIMARY` spelling explicitly; no speculative fallback
creates an ambiguous duplicate owner. The default forwarding/listen tuple policy rejects
duplicate children before sending `SESSION ADD`.

The `sam-conformance` command tests session create/close probes against a supplied bridge.
Its `probe_ok` rows do not claim payload exchange or broad router compatibility. Keep
live interoperability output separate from mock bridge tests.
