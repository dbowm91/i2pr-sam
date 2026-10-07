# SAM v3 reference freeze

Frozen for the initial implementation line on 2026-10-07. These sources are used as
normative text or as independently checked compatibility observations; no implementation
source was copied.

## Normative material

- SAM v3: <https://i2p.net/en/docs/api/samv3/> (retrieved 2026-10-07; site marks the
  document accurate for I2P 0.9.70 and updated 2026-07).
- Proposal 163, DATAGRAM2/3: <https://geti2p.net/spec/proposals/163-datagram2-datagram3>
  (retrieved 2026-10-07).
- I2P datagram specification: <https://geti2p.net/spec/datagrams> (retrieved
  2026-10-07).

## Deployed behavior targets

| Project | Pinned revision | Role |
|---|---|---|
| Java I2P (`i2p/i2p.i2p`) | `a629ec7c9c675dd252d005fb881efd92e8e6ba27` (repository HEAD on 2026-10-07) | deployed interoperability target |
| i2pd (`PurpleI2P/i2pd`) | `d147bb0fd6789c75dc1c4d70c4f79b151a552d53` (repository HEAD on 2026-10-07) | deployed interoperability target |
| i2pr (`dbowm91/i2pr`) | `4f4e98f5a0241355af5099c73065088dede1b1de` (repository HEAD on 2026-10-07) | deployed interoperability target |

The revision values are Git remote HEADs observed with `git ls-remote` on the freeze
date, not claims that these projects were run locally. Live test evidence is recorded in
milestone closures, separately from this source pin.

## 2026-10-07 pin re-verification (milestone 005)

The three deployed-behavior pins were re-observed with `git ls-remote` on 2026-10-07 during
milestone 005 and are unchanged from the values above:

```
git ls-remote https://github.com/i2p/i2p.i2p.git HEAD  -> a629ec7c9c675dd252d005fb881efd92e8e6ba27
git ls-remote https://github.com/PurpleI2P/i2pd.git HEAD -> d147bb0fd6789c75dc1c4d70c4f79b151a552d53
git ls-remote https://github.com/dbowm91/i2pr.git HEAD  -> 4f4e98f5a0241355af5099c73065088dede1b1de
```

The values are also recorded declaratively in `scripts/interop/routers.json` so the harness
and the documentation cannot drift apart.

## Normative clarifications established by milestone 005

Milestone 005 re-read the frozen documents and recorded the clauses the implementation now
depends on. These are the exact passages the corrective work was written against.

1. **Control-socket STREAM ACCEPT announces the peer separately from payload.** After
   `STREAM STATUS RESULT=OK`, a non-silent accept is followed by `$destination`, optional
   `FROM_PORT=`/`TO_PORT=` lines, and a blank line, and only then payload. `STREAM CONNECT`
   has no such block. Milestone 004 started payload framing at the status line, so the peer
   block was delivered to applications as data and `remote_destination()` could never be
   populated.
2. **Success replies carry the private key.** `SESSION CREATE` answers
   `RESULT=OK EXPIRES=... DESTINATION=$privkey`. The client retains that value only in
   memory and never renders, logs, or publishes it.
3. **`NAMING LOOKUP NAME=ME` is the documented identity path.** The reply carries
   `VALUE=$destination` for the calling socket's session, which is how a router-generated
   Destination is resolved without retaining key material.
4. **DATAGRAM/RAW protocol numbers 6, 17, 19 and 20 are reserved.** The specification marks
   them "not allowed" for `STYLE=RAW`, and a RAW subsession may not set
   `LISTEN_PROTOCOL=6`.
5. **The v1/v2-compatible control-socket datagram mechanism is send-and-receive with raw
   bytes.** `DATAGRAM SEND`/`RAW SEND` take `SIZE=$n` followed by exactly `n` raw bytes -
   not base64 - and deliveries arrive as `DATAGRAM RECEIVED`/`RAW RECEIVED` lines followed
   by `SIZE` raw bytes. The legacy documentation shows no `ID=` field; the current runner
   implementations require one, and the client sends it.
6. **Raw forwarding metadata is conditional.** The FROM_PORT/TO_PORT/PROTOCOL block appears
   only when the session was created with `HEADER=true` (SAM 3.2+). Without it a forwarded
   RAW datagram is bare payload, and inventing metadata for it misreports the wire.
7. **Primary sessions carry no datagram routing options.** `PORT`, `HOST`, `FROM_PORT`,
   `TO_PORT`, `PROTOCOL`, `LISTEN_PORT`, `LISTEN_PROTOCOL`, and `HEADER` belong to
   subsessions.

## 2026-10 protocol update

The current SAM documentation was retrieved 2026-10-07 and identifies itself as updated
2026-07 / accurate for router API 0.9.70. It incorporates Proposal 163 but still says no
DATAGRAM2/3 implementations are known. This is a current compatibility warning, not a
reason to treat the new styles as universally available. The test matrix therefore keeps
each style independently observable and does not infer support from SAM 3.3.

## Compatibility observation

i2pd issue [#2303](https://github.com/PurpleI2P/i2pd/issues/2303) is a compatibility
input only. The issue is closed; maintainer comments state i2pd retains `MASTER` spelling
for SAM 3.3 and challenge changing it without a SAM version change. The issue does not
establish broad support for all child styles. This behavior is not normative SAM
authority. The client keeps shared-session dialect selection explicit and capability
state independent of negotiated version.

## Behavioral/API oracles

No revisions are frozen for go-sam-go, Yosemite, sam3, or sam-forwarder because their
source was not inspected for implementation. If a later milestone uses them as API or
behavioral references, it must record exact revisions in its closure. They are never
source material.

## Clean-room rule

Only published specifications and independently authored test cases define the wire
codec. Existing libraries and routers may be observed to discover external behavior, but
their implementation text, tests, or distinctive code structure must not be copied.
