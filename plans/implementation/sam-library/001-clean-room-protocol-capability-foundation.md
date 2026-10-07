# SAM Library Milestone 001 — Clean-room protocol and capability foundation

Status: ready for handoff

Repository baseline: `8403e741be5daa23fe412ce7ebe5f280c5644665`

Source roadmap:

- `plans/subsystems/sam-library-roadmap.md#001--clean-room-protocol-and-capability-foundation`

Long-term requirements:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`

Applicable ADRs:

- `plans/adrs/ADR-0001-clean-room-reference-and-provenance-policy.md`
- `plans/adrs/ADR-0002-layered-client-runtime-and-bindings-ownership.md`

Primary class: invariant + infrastructure

## 1. Objective

Create the first compilable Rust workspace and a runtime-neutral SAM protocol/state
foundation that later networking code can depend on without embedding string parsing,
router quirks, or unbounded input handling in session logic.

This milestone also freezes the exact normative/reference baseline used by the initial
implementation and defines the capability model needed for non-uniform SAM 3.x routers.

It does **not** open a network socket.

## 2. Why this milestone is ready

The repository is empty apart from planning. There are no implementation dependencies.

The architectural boundary is decided by ADR-0001/0002. The official SAM v3
documentation is the normative starting point. Current deployed behavior is known to be
non-uniform enough that version negotiation cannot be the only capability model; in
particular, shared-session naming differs between PRIMARY and MASTER implementations and
Datagram2/3 deployment has changed faster than older documentation prose.

## 3. Current implementation evidence

There is no code yet.

Registration-time research establishes:

- SAM v3 lines use command/subcommand plus key/value fields with v3.2 quoting/escaping
  and UTF-8 behavior;
- HELLO chooses a protocol version but optional behavior remains implementation-specific;
- SAM includes utility, Destination, naming, session, stream, datagram/raw, and shared
  session command families;
- DATAGRAM2 and DATAGRAM3 have source-authentication semantics distinct from both legacy
  DATAGRAM and RAW;
- shared sessions are semantically one Destination with children, but deployed routers
  may use PRIMARY or MASTER spelling.

Milestone 001 must pin exact reference revisions before deriving implementation fixtures.

## 4. Invariants that must not regress

- Protocol crate owns no sockets, Tokio, tasks, timers, DNS, filesystem, process state,
  environment access, or global mutable state.
- No peer/router-controlled line causes an unbounded allocation.
- Wire parsing never relies on `split_whitespace()` where quoted SAM values are legal.
- Raw input is retained only as needed to parse; errors do not echo secret-bearing
  Destination/private-key/auth values.
- Duplicate identity-bearing fields are rejected deterministically unless a command's
  specification explicitly permits repetition.
- Unknown optional fields can be preserved/ignored according to command semantics without
  becoming silently accepted required behavior.
- Negotiated version and optional capabilities are separate types.
- DATAGRAM3 source identity is represented as unverified.
- RAW has no peer/source identity.
- No implementation source is copied from reference projects.

## 5. Scope

### In scope

- initialize Cargo workspace with Rust edition 2024 and MSRV 1.89 to match current i2pr;
- create `crates/i2pr-sam-proto`;
- create a minimal `crates/i2pr-sam` shell that depends only on the protocol crate and
  exposes no networking behavior yet;
- freeze normative and behavioral references under `specs/references/`;
- bounded SAM tokenizer/parser/serializer;
- typed version, command, reply, result/error, port/protocol, session-style, Destination
  text, session-ID, and option representations needed by the initial roadmap;
- command legality/state model sufficient to prevent obvious phase misuse;
- capability model with supported/unsupported/unknown states independent of version;
- secret redaction rules;
- property/negative tests.

### Explicitly out of scope

- TCP/TLS connections;
- Tokio runtime;
- HELLO over a real bridge;
- Destination cryptography;
- session creation against a router;
- STREAM data transfer;
- datagram sockets;
- PRIMARY/MASTER probing;
- Python/C/blocking APIs;
- service-tunnel adapter;
- package publication.

## 6. Required production changes

### Workspace

Create a workspace with at least:

```text
crates/i2pr-sam-proto
crates/i2pr-sam
```

Use `resolver = "2"`, edition 2024, MSRV 1.89, and workspace lint policy including
`unsafe_code = "deny"`. Do not invent repository license metadata; this repository has
no selected license at registration time.

### Reference freeze

Add `specs/references/sam-v3-reference-freeze.md` recording:

- official SAM v3 documentation URL and retrieval date;
- relevant I2P datagram/proposal references;
- exact Java I2P revision/release selected for later interop;
- exact i2pd revision/release;
- exact i2pr revision;
- exact reference revisions for go-sam-go, Yosemite, sam3, and sam-forwarder if inspected;
- role of each source: normative, deployed interop, or behavioral/API oracle;
- explicit no-copy rule.

The freeze should note i2pd issue #2303 and its final disposition as a compatibility input,
not normative authority.

### Protocol lexer/parser

Implement bounded parsing for:

- command/subcommand words;
- key/value tokens;
- quoted values;
- v3.2 backslash/quote escaping;
- UTF-8 rules where applicable;
- line termination;
- optional bare command forms;
- PING/PONG arbitrary trailing data where specified.

Define named limits for line length, token count, key length, value length, and decoded
Destination/private-key strings. Derive conservative values from protocol maxima and
real router behavior; document the derivation.

Do not build typed commands by first allocating an unbounded map.

### Typed protocol model

At minimum define types sufficient to model:

- HELLO VERSION / HELLO REPLY;
- DEST GENERATE / DEST REPLY;
- NAMING LOOKUP / NAMING REPLY;
- SESSION CREATE / ADD / REMOVE / STATUS;
- STREAM CONNECT / ACCEPT / FORWARD / STATUS;
- legacy DATAGRAM/RAW control-socket send/receive forms where specified;
- style enum including STREAM, DATAGRAM, RAW, DATAGRAM2, DATAGRAM3, and shared-owner
  dialect markers;
- typed SAM result values while preserving unknown result strings for diagnostics.

It is acceptable for commands planned for M003 to be typed but not executable.

### Capability model

Create a representation such as:

```text
SamCapabilities
  negotiated_version
  stream
  datagram
  raw
  datagram2
  datagram3
  shared_master
  shared_primary
  session_add_remove
  naming_lookup_options
  authentication
  ping_pong
  ...
```

Each optional capability needs at least Unknown / Supported / Unsupported. Do not derive
Supported solely from `HELLO VERSION=3.3`.

Separate static version-syntax requirements from observed router capability state.

### State legality

Represent enough state to reject impossible transitions before networking exists:

- pre-HELLO;
- negotiated utility/control;
- ordinary session-control;
- shared-owner control;
- child/subsession relation;
- STREAM connection transitioning from command framing to data phase;
- terminal/closed.

This may be a test-oriented transition model rather than a public API.

## 7. Ordered work packages

### Work package A — reference/provenance freeze

Pin sources, classify authority, and record current compatibility observations.

Acceptance evidence:

- reference freeze committed;
- every behavioral fixture cites either a normative rule or an independently authored
  test case.

### Work package B — workspace and boundary guards

Create crates/lints and a checker ensuring `i2pr-sam-proto` has no runtime/network
dependencies.

Acceptance evidence:

- dependency tree contains no Tokio/socket/runtime crate in protocol core;
- positive control proves the checker would catch one.

### Work package C — bounded wire syntax

Implement tokenizer, escaping, serialization, and limits.

Acceptance evidence:

- round-trip/property tests;
- malformed quoting/escaping/UTF-8/max+1 cases fail without panic or unbounded allocation.

### Work package D — typed commands/replies

Implement typed conversions and redacted errors.

Acceptance evidence:

- official examples parse/serialize as expected;
- duplicate/unknown/required-field behavior is tested.

### Work package E — state/capability model

Implement version syntax constraints plus tri-state optional capabilities and phase
legality.

Acceptance evidence:

- tests prove a 3.3 negotiated version can still report DATAGRAM2/PRIMARY unknown or
  unsupported;
- illegal data/control transitions fail deterministically.

## 8. Failure, cancellation, restart, and contention semantics

There is no runtime concurrency in M001.

Parsing is all-or-error for one bounded frame. No parser state may retain unbounded partial
input across calls. If incremental parsing is used, buffer ceilings apply before append.

State-machine errors leave the semantic state unchanged unless the transition explicitly
enters Terminal/Closed.

## 9. Compatibility and migration

No backward compatibility exists yet.

The protocol model should preserve unknown result strings/options where safe so later
router-specific extensions do not require parser rewrites. This is not permission to
execute unknown commands.

## 10. Required tests

### Focused unit tests

- tokenizer and escaping;
- version comparisons/ranges;
- ports/protocol bounds;
- session ID validation;
- command/reply typed conversion;
- result mapping;
- redaction.

### Property tests

- serialize(parse(x)) canonicalization for generated legal lines;
- parser never panics on bounded arbitrary bytes;
- max-1/max/max+1 for named limits.

### State tests

- HELLO before session commands;
- session child ownership;
- STREAM success moves connection to data phase;
- commands rejected in data phase;
- primary close invalidates children in the abstract model.

### Security/negative tests

- secret values absent from Debug/Display errors;
- duplicate identity fields rejected;
- oversized line/token/value rejected before large allocation.

## 11. Required verification commands

The implementation may establish exact package names/scripts, but closure must include at
least:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
cargo tree -p i2pr-sam-proto
```

Also run the boundary/provenance checker added by this milestone and any property/fuzz
smoke lane it establishes.

## 12. Documentation updates

- root README implementation status;
- protocol/reference freeze;
- crate-level architecture docs;
- roadmap/registry status;
- any compatibility note discovered during source pinning.

## 13. Acceptance criteria

1. Workspace compiles on MSRV 1.89 and current stable.
2. Protocol crate has no socket/runtime ownership.
3. Reference freeze names exact sources/revisions.
4. Bounded parser covers the command families required by M002/M003.
5. Version and optional capabilities are distinct.
6. STREAM data-phase transition is explicit.
7. DATAGRAM3/RAW identity semantics cannot be mistaken for authenticated peer identity.
8. Secret-bearing input is redacted.
9. Negative bounds/property tests pass.
10. No real networking behavior is claimed.

## 14. Stop conditions

Stop and register a corrective/ADR if:

- official syntax is internally contradictory in a way that changes the public model;
- deployed behavior requires copying implementation-specific syntax not representable as
  a narrow compatibility extension;
- an apparently protocol-neutral type requires socket/runtime ownership;
- exact reference provenance cannot be established;
- a license decision becomes necessary to proceed with packaging/publication.

## 15. Closure evidence required

Record exact source freeze, dependency tree, parser/state test counts, named limits and
their derivation, positive-control boundary result, MSRV/stable commands, and any
spec/deployed discrepancy discovered.

## 16. Handoff notes

Do not optimize API ergonomics prematurely. M001's job is to make invalid wire/state
behavior difficult to represent and to give M002 a stable typed substrate.
