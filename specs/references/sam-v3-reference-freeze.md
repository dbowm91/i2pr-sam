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
