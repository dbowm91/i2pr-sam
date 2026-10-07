# i2pr-sam Terminology and Domain Model

Status: canonical.

## Protocol terms

**Bridge** — a SAM server exposed by an I2P router or router-adjacent process.

**Client connection** — one local transport connection from this library to a SAM bridge.

**Negotiated version** — the SAM version selected by HELLO. It constrains syntax but is
not equivalent to a complete feature set.

**Capability** — a feature independently known/proven usable on the selected bridge,
such as PRIMARY-style creation, MASTER-style creation, DATAGRAM2, DATAGRAM3, or naming
options.

**Router profile** — observed compatibility facts associated with a bridge/router
implementation. Profiles may guide behavior but must not override direct protocol
evidence.

## Identity terms

**Destination** — public I2P identity/address material.

**Private Destination** — Destination plus private key material required to own it.

**Destination ownership domain** — the lifetime/linkability domain in which sessions
intentionally share one Destination.

**Session ID** — SAM's client-visible session nickname/identifier. It is not an I2P
Destination and must not be treated as authenticated peer identity.

## Session terms

**Ordinary session** — one STREAM, DATAGRAM, RAW, DATAGRAM2, or DATAGRAM3 SAM session
owning its own Destination/tunnel set.

**Shared session** — library-level semantic abstraction for one Destination/tunnel set
with multiple child/subsessions. The wire owner may be spelled PRIMARY or MASTER.

**Primary/Master dialect** — router-facing syntax used to establish a shared session.
It is an interoperability detail, not a second semantic API.

**Subsession / child session** — STREAM/datagram child attached to a shared session.

## Stream terms

**Control phase** — command/reply framing is active.

**Data phase** — after successful STREAM CONNECT/ACCEPT, the connection is an opaque byte
stream. SAM command parsing on that connection is forbidden.

**Authenticated remote Destination** — the peer Destination supplied by successful
STREAM semantics. Local TCP addresses and session nicknames are not substitutes.

## Datagram terms

**Authenticated datagram source** — a source whose SAM/I2P datagram format provides
cryptographic source authentication, e.g. legacy repliable DATAGRAM or DATAGRAM2.

**Unverified source hash** — DATAGRAM3's reply-capable source hash. It is explicitly not
authenticated.

**Raw datagram** — protocol payload without a source identity supplied by the SAM
datagram format.

**I2P port** — SAM/I2CP port metadata. It is unrelated to local TCP/UDP port selection.

## Higher-level terms

**Service-tunnel policy core** — the public, runtime-neutral `i2pr-service-tunnels`
crate consumed by a future adapter. It owns policy/filtering, not SAM I/O.

**Tunnel adapter** — maps validated service-tunnel policy into SAM sessions and local
socket lifecycle.

**Tunnel daemon** — standalone process that owns config, identity persistence, runtime,
reconnect, local listeners/targets, management API, and adapter lifecycle.

**Binding** — C ABI or Python surface wrapping canonical Rust semantics.
