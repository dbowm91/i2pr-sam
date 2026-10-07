# API migration notes

The library is pre-1.0. Milestones 005 and 006 changed the public surface where the old
surface could misstate protocol behaviour. Every change below is intentional; none of them
is cosmetic.

## Milestone 005 - protocol correctness

### Session identity replaces the request token

```rust
// before
let session = client.create_shared_session("TRANSIENT", "svc", SharedDialect::Primary, &[]).await?;

// after
let session = client
    .create_shared_session(&SessionDestination::Transient, "svc", SharedDialect::Primary, &[])
    .await?;
let identity: &SessionIdentity = session.identity();
```

`SessionDestination` has variants `Transient`, `Generated`, `Imported(Destination)` and
`WithKey { public, secret }`. A shared session now fails with `SamError::IdentityUnavailable`
when the router does not reveal a concrete Destination, instead of silently returning the
`TRANSIENT` token.

### Accept reports the peer it learned from the wire

`SamStream::remote_destination()` previously always returned `None` on accept, because the
peer identity block after `STREAM STATUS RESULT=OK` was never consumed. Both
`remote_destination()` and the new `peer() -> Option<&StreamPeer>` now report the announced
peer, and payload framing starts after the block. New `accept_with(silent)` selects
`SILENT=true`.

### Datagram sessions name their transport

```rust
// before - always UDP forwarding
let session = client.create_datagram_session(&destination, id, SessionStyle::Datagram, &[]).await?;

// after - explicit, and the v1/v2-compatible mode is opt-in
use i2pr_sam::DatagramTransport;
let session = client
    .create_datagram_session_with(&destination, id, SessionStyle::Datagram, DatagramTransport::ControlSocketV1, &[])
    .await?;
```

Requesting `ControlSocketV1` for DATAGRAM2/DATAGRAM3 returns `SamError::Unsupported` before
any command is sent. New `dropped_datagrams()` and `queued_datagrams()` expose bounded-queue
behaviour on that transport.

### Shared children carry the owner's identity

`SharedChild::identity() -> &SessionIdentity` returns the owner's concrete Destination.
`SharedChild::style()` returns the wire spelling and `style_kind()` returns the typed style.

## Milestone 006 - API and verification semantics

### Retry policy renamed and bounded globally

`ReconnectPolicy` is now `ConnectRetryPolicy`; the old name remains as a deprecated type
alias. Two fields were added:

```rust
let policy = ConnectRetryPolicy::bounded(5, Duration::from_millis(100), Duration::from_secs(5), Duration::from_secs(30))?;
policy.max_concurrent_admissions = 8;   // process-wide budget
policy.saturation = RetrySaturation::Reject;
```

Saturation produces `SamError::RetryAdmissionSaturated`, classified as
`FailureClass::ResourceExhausted`. `SamClient::connect_with_clock(config, policy, clock)`
accepts a `RetryClock`, so retry budgets are testable without wall-clock waits.

### Session options are typed

`SessionOptions` validates keys at construction, rejects control characters in values, and
deduplicates repeated keys. Existing `&[(String, String)]` call sites still compile through
`Deref`; reserved-key collisions were already rejected at command assembly and remain so.

```rust
use i2pr_sam::SessionOptions;
let options = SessionOptions::builder()
    .option("sam.udp.host", "127.0.0.1")
    .build(&["STYLE", "ID", "DESTINATION"])?;
```

### Lookup distinguishes Destinations from hash addresses

`SamClient::lookup()` still returns the raw reply for callers that need it.
`SamClient::lookup_destination()` returns a typed `Destination`, and the new
`SamClient::resolve_peer()` returns a `PeerTarget`:

```rust
match client.resolve_peer("stats.i2p").await? {
    i2pr_sam::PeerTarget::Destination(destination) => { /* full key material */ }
    i2pr_sam::PeerTarget::Base32Hash { value, .. } => { /* a name, not a Destination */ }
}
```

Using a hash address where a Destination is required is now a typed refusal rather than a
confusing protocol error.

### Ports are typed in the blocking facade

`BlockingClient::create_stream_session`, `BlockingSharedChild::connect`, and the datagram
send methods take `Option<Port>` instead of `Option<u16>`. `BlockingClient::generate_destination`
returns `GeneratedDestination` instead of `(String, SecretDestination)`.

### New error and failure variants

`SamError::Unsupported(String)` separates a capability verdict from a rejection, and
`SamError::RetryAdmissionSaturated` separates resource exhaustion. Both map to new
`FailureClass` variants, so `classify_failure` remains exhaustive over the vocabulary.

### Resource accounting

`i2pr_sam::resource_usage() -> ResourceUsage` reports live sessions, live streams, peak
stream concurrency, open control sockets, and dropped datagrams. Counters are observability
only and never gate a protocol decision.

## Removed behaviour

Nothing was removed outright. The `TRANSIENT`-as-identity result, the always-`None` accept
peer, and the unconditional `HEADER=true` RAW policy were replaced by correct behaviour, and
this note plus `docs/client-usage.md` describe the replacement path for each.