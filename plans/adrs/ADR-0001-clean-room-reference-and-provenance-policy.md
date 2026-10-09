# ADR-0001 — Clean-room SAM reference and provenance policy

Status: accepted.

## Context

i2pr-sam must interoperate with a protocol that has both normative documentation and
deployed implementation differences. Existing SAM libraries provide valuable API and
behavior examples, but copying implementation source would create unnecessary provenance
and maintenance risk.

## Decision

The implementation is clean-room.

Normative behavior comes from published I2P SAM/I2CP/datagram specifications and
proposals. Deployed Java I2P, i2pd, I2P+, and i2pr are interoperability targets and
behavioral oracles.

Existing client libraries — including `go-i2p/go-sam-go`, `go-i2p/sam3`,
`yosemite`, and `sam-forwarder` — may be studied for API shape, test scenarios, and
observable behavior. Their source is not copied or mechanically translated.

Implementation plans freeze exact reference revisions before deriving tests from observed
behavior.

## Consequences

- Protocol tests should be expressed from specifications or independently authored
  fixtures.
- Compatibility quirks require a citation/reference and observed evidence.
- A future provenance exception requires a new ADR.
- Similarity of public API concepts is acceptable when dictated by the protocol/domain;
  source-level reproduction is not.
