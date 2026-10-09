# SAM service-tunnel adapter

`i2pr-sam-service-tunnels` composes the `i2pr-sam` STREAM client with the public
`i2pr-service-tunnels` policy core pinned at
`f0fb74a8582d6077a7c5db49d613699688115bad`. It calls the core's validation, group planning,
HTTP parsing/filtering, and authenticated-peer access/rate policy APIs; it does not copy
those policies.

The initial adapter supports enabled `GenericClient`, `GenericServer`, and `HttpServer`
profiles with loopback TCP server targets. It rejects other profiles and non-default
Destination crypto policy before opening the SAM connection. Client destinations are
resolved through SAM. Server streams are accepted with peer authentication enabled, and
the authenticated Destination hash is passed to the core's access and rate policies.

Equal explicit group identities share one SAM owner. Dedicated groups stay separate.
Persistent groups require an application-provided `DestinationIdentityStore`; this crate
does not select a storage location or write key material itself. The store must protect
private Destination material and return only non-secret error categories.

The adapter owns one concurrency permit per active service connection. HTTP request heads
are parsed and filtered through the core before forwarding; forwarding uses the profile's
byte-buffer ceiling and connect/read/write/shutdown deadlines. Callers own local listener
lifecycle and decide when to accept connections and call `forward_server_stream`.

SOCKS, IRC, CONNECT, Streamr/datagram services, non-loopback targets, full HTTP body
filtering, daemon configuration, and key storage are outside this initial adapter slice.
The mock-bridge tests establish adapter behavior only and make no live-router claim.
