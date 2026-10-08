# M011 provisioned-router attempt — 2026-10-08

Qualification code revision: `75d1a13f7a2f09aea720c7489ec8d7ac4ab67317`.

## Router versions and bridges

- Java I2P package `2.13.1-1~ubuntu4`; router runtime log reports
  `2.13.0-0-1~ubuntu4`. SAM TCP endpoint: `127.0.0.1:7656`.
- i2pd source revision `d147bb0fd6789c75dc1c4d70c4f79b151a552d53`, built with CMake as
  `2.61.0-739-gd147bb0f` (`0.9.70`). It used the existing i2pd router database. SAM TCP
  endpoint: `127.0.0.1:19856`.

Both endpoints answered `HELLO VERSION MIN=3.0 MAX=3.3` with `HELLO REPLY`. The harness
reported both bridge preflights successful and reached `sam-conformance`.

## Full-plan commands

```text
python3 scripts/interop/qualify.py --router i2pd --endpoint 127.0.0.1:19856 --peer-endpoint 127.0.0.1:7656 --plan full --skip-udp-probe --artifact-dir /tmp/i2pr-sam-m011-i2pd-final --matrix-out /tmp/i2pr-sam-m011-i2pd-final/matrix.csv --timeout 300
python3 scripts/interop/qualify.py --router 'Java I2P' --endpoint 127.0.0.1:7656 --peer-endpoint 127.0.0.1:19856 --plan full --skip-udp-probe --artifact-dir /tmp/i2pr-sam-m011-java-final --matrix-out /tmp/i2pr-sam-m011-java-final/matrix.csv --timeout 300
```

| Router | pass | unsupported | fail | not_run | SAM |
|---|---:|---:|---:|---:|---|
| Java I2P | 0 | 0 | 0 | 9 | 3.3 |
| i2pd | 0 | 1 | 0 | 8 | 3.3 |

i2pd's unsupported row is PRIMARY shared style, reported as
`I2P_ERROR MESSAGE="Unknown STYLE"`; i2pd retains MASTER. All payload rows were `not_run`
because the conformance clients could not resolve concrete Destinations. The MASTER shared
row was also `not_run` for unavailable identity. i2pd's router log reported
`Can't create inbound tunnel, no peers available`.

## UDP advisory probe

```json
{"udp_egress": true, "udp_high_port_egress": null, "i2p_router_udp_capable": null, "reachability": "reachable", "probe_target": "1.1.1.1:53; 1.1.1.1:443; 1.1.1.1:34567", "reachable": ["dns_53 at 1.1.1.1:53"], "verdict": "reachable", "error": null}
```

The only observed request/reply was DNS on port 53. High-port reachability is unknown; the
probe does not establish an egress policy.

## Result

Both local routers and SAM bridges were provisioned, but this host did not provide usable
cross-router tunnels or peer identity resolution. No live payload compatibility result is
claimed. The JSON artifacts and per-router/combined matrices in this directory validate
against the conformance schema and preserve the `not_run` rows as environmental evidence.

An initial Ubuntu i2pd package process (`2.49.0-1build3`) aborted with allocator heap
corruption after joining the network. The M011 artifact uses the pinned source build above,
not that package process.

## 2026-10-08 — initial Java same-router probe before a service tunnel was active

To avoid attempting a Java-I2P-to-i2pd tunnel, Java I2P was tested with two independent
SAM clients connected to the same bridge (`127.0.0.1:7656` used for both `--endpoint` and
`--peer-endpoint`). This topology is supported by the runner and needs no cross-router
tunnel. The Java SAM bridge was temporarily enabled in its local client configuration for
this attempt; the original `startOnLoad=false` and 120-second startup delay were restored
afterward, and the router was stopped.

The bridge passed SAM 3.3 negotiation, but SAM session creation did not yield a concrete
Destination. The runner reported `SAM operation timed out`; the router log recorded
`SAM socket closed while waiting for tunnels to build` and an interrupted
`I2PSessionImpl.connect`. The router console reported `Network: Firewalled`. The resulting
schema-valid artifact, `java-i2p-same-router-stream.json`, has `local_identity=null` and
the STREAM row `not_run` (`peer_identity_unavailable`, zero bytes exchanged). A repeat
with `--control-timeout 120` produced the same outcome.

For a same-router payload row to pass, Java I2P first needs to establish a SAM session and
return a concrete Destination, then build usable local inbound/outbound tunnels for the two
session Destinations so STREAM (or datagram) payload can be exchanged. Both clients may
connect to the same SAM bridge; a second router is not needed. This initial configuration
did not finish session/tunnel setup. The follow-up below uses the router's configured
webserver tunnel as the known peer destination.

## 2026-10-08 — SAM HTTP request to Java I2P's local server tunnel

The existing Java I2P `I2P webserver` tunnel was enabled temporarily. The local server
endpoint was `127.0.0.1:7658`; its router-reported public address was
`hqlkjlb3ehnwjd2i64ikghjlmdrz3ws43yc2nehhdilvgwwpxjuq.b32.i2p`. Two router-local
destinations were not needed: the SAM client resolved that concrete address and opened a
STREAM session directly to the HTTP service inside the same router.

Reproduction from the repository root:

```text
cargo run --offline -p i2pr-sam --example sam-http-get -- \
  hqlkjlb3ehnwjd2i64ikghjlmdrz3ws43yc2nehhdilvgwwpxjuq.b32.i2p
```

Observed result:

```text
sam=connected target=hqlkjlb3ehnwjd2i64ikghjlmdrz3ws43yc2nehhdilvgwwpxjuq.b32.i2p
name_resolution=ok
session=created identity=false
response_bytes=1186
response_header=HTTP/1.1 200 OK
http_response=valid
```

The bridge negotiated SAM 3.3 on Java I2P runtime `2.13.0-0-1~ubuntu4`. This is a successful
live SAM STREAM request and response through Java I2P to a valid
Destination, hosted by a server tunnel on the same router. The earlier timeout was caused
by probing before an addressable service tunnel was running; it did not establish that
same-router SAM requests are unavailable. The router was stopped and the SAM/server-tunnel
`startOnLoad` settings were restored after the probe.
