# SAM Library Milestone 005 — Live interoperability and protocol-closure corrective

Status: executed; conditionally closed

Closure: `plans/closure/sam-library/005-status.md`
Implementation revision: `2439485a63319ae8501ac3ccad789f886a4465e0`
Residual: live multi-router payload matrix, registered as milestone 011.

Repository baseline: `d7ee20fff63ba4421e957455c10d94c23d4529aa`

Corrects:

- `plans/closure/sam-library/002-status.md` — conditionally closed
- `plans/closure/sam-library/003-status.md` — conditionally closed
- protocol/interoperability portions of `plans/closure/sam-library/004-status.md`

Source roadmap:

- `plans/subsystems/sam-library-roadmap.md`

Long-term requirements:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`

Applicable ADRs:

- `plans/adrs/ADR-0001-clean-room-reference-and-provenance-policy.md`
- `plans/adrs/ADR-0002-layered-client-runtime-and-bindings-ownership.md`

Primary class: capability + invariant + corrective qualification

## 1. Objective

Convert the M002/M003 implementation from mock-qualified to protocol-qualified before any
foreign-language binding or service-tunnel adapter is allowed to depend on it.

The corrective must:

1. execute real SAM traffic against current Java I2P and i2pd and execute every currently
   advertised i2pr SAM row available at implementation time;
2. prove STREAM payload exchange, not merely session creation;
3. prove shared-session identity using the concrete router-generated I2P Destination (or
   canonical hash), never the literal request token `TRANSIENT`;
4. exercise payload traffic through shared STREAM and datagram children;
5. implement or explicitly and permanently narrow the ordinary DATAGRAM/RAW control-socket
   modes that M003 planned but did not implement;
6. validate DATAGRAM2/DATAGRAM3 independently of negotiated SAM version;
7. disposition PRIMARY versus MASTER from live evidence;
8. upgrade the conformance runner so “pass” means the requested semantic operation
   actually occurred.

This milestone does not add bindings, daemon behavior, or i2pr service-tunnel policy.

## 2. Why this milestone is ready

M001 is strictly closed. M002–M004 have substantial local implementations and truthful
conditional closures. The corrective is therefore evidence-driven rather than speculative.

Repository evidence already identifies the exact gaps:

- Java I2P, i2pd, and i2pr live rows are all `not_run`;
- the shared mock compares the stored string `"TRANSIENT"` rather than a concrete
  Destination;
- ordinary datagrams currently rely on UDP forwarding;
- direct v1/v2-compatible `DATAGRAM SEND` / `RAW SEND` and control-socket receive are
  absent;
- the existing conformance runner treats successful create/close as a successful probe;
- shared PRIMARY/MASTER semantics have no live payload proof.

Current SAM documentation states:

- ordinary DATAGRAM/RAW may use local UDP forwarding;
- if a forwarding PORT is omitted, ordinary incoming datagrams may use the control socket
  in v1/v2-compatible mode;
- ordinary DATAGRAM/RAW have v1/v2-compatible direct SEND commands;
- DATAGRAM2/DATAGRAM3 do **not** use those v1/v2-compatible direct-send/receive semantics;
- primary/subsessions do **not** use v1/v2 datagram/raw send/receive;
- shared children use the primary Destination and tunnel set;
- deployed implementations differ on PRIMARY versus MASTER spelling.

Those distinctions must be encoded and tested rather than generalized away.

## 3. Current implementation evidence

At baseline:

- `i2pr-sam-proto` provides bounded syntax, state, capability, Destination, and datagram
  trust types.
- `i2pr-sam` provides HELLO, Destination generation, naming, ordinary STREAM,
  UDP-forwarded datagrams, explicit MASTER/PRIMARY shared owners, child add/remove, and
  a basic conformance executable.
- `i2pr-sam-blocking` wraps the async implementation.
- deterministic mock tests exist for STREAM, one ordinary DATAGRAM UDP path, shared
  add/remove, malformed/oversized HELLO, retry classification, and 100 STREAM session
  create/close cycles.
- current live qualification attempts failed before protocol negotiation because no SAM
  bridge was running.
- the public capability model correctly does not infer optional feature support from
  `HELLO VERSION=3.3`.

This plan preserves the good architecture and closes the missing protocol evidence.

## 4. Invariants that must not regress

- negotiated version is not feature support;
- DATAGRAM1/DATAGRAM2 authenticated source identity is distinct from DATAGRAM3 unverified
  source hash and RAW no-source semantics;
- shared-session child traffic uses one concrete Destination/linkability domain;
- a primary/master owner is never used for payload commands;
- ordinary v1/v2-compatible datagram commands are never used for DATAGRAM2/3;
- ordinary v1/v2-compatible datagram commands are never used on shared subsessions;
- no automatic PRIMARY/MASTER fallback may create a duplicate live owner;
- every router-specific difference remains centralized in capability/compatibility logic;
- no private Destination/auth secret reaches default logs, conformance JSON, or errors;
- all control reads, datagram reads, child registries, and test harness queues stay bounded;
- reference router source remains behavioral evidence, not copied implementation material.

## 5. Scope

### In scope

- reproducible router-enabled interoperability harness;
- exact reference/pin refresh at implementation start;
- ordinary STREAM live qualification;
- concrete Destination identity proof;
- ordinary DATAGRAM1 and RAW UDP forwarding;
- ordinary DATAGRAM1 and RAW v1/v2-compatible control-socket send/receive;
- DATAGRAM2 and DATAGRAM3 UDP/control behavior exactly as current spec/router support
  permits;
- shared MASTER/PRIMARY creation, add/remove, STREAM payload, datagram payload, concrete
  same-Destination proof, sibling survival, and owner teardown;
- capability observation and unsupported classification;
- conformance runner semantic upgrades;
- protocol fixes revealed by live routers;
- deterministic regression tests for every live defect found.

### Explicitly out of scope

- C ABI or Python bindings;
- service-tunnel adapter;
- tunnel daemon/config/WebUI;
- automatic session identity recovery;
- router implementation changes in Java I2P/i2pd/i2pr except separately filed upstream
  work;
- treating an unsupported optional feature as a test failure when the router truthfully
  does not claim it;
- publication.

## 6. Required production changes

### Router-enabled qualification harness

Add a repeatable harness under `scripts/` and/or `tests/interop/` that can drive a
configured SAM bridge without manual editing.

The harness must support at least:

- Java I2P;
- i2pd;
- i2pr.

Each run records:

- router label;
- exact release/version/commit;
- bridge endpoint;
- negotiated SAM version;
- operation/capability;
- shared dialect where applicable;
- concrete local Destination hash where applicable;
- result: pass / unsupported / fail / not_run;
- diagnostic category;
- artifact timestamp.

Provisioning may differ per router. Do not hide unavailable infrastructure behind a
passing skip. Ordinary unit CI may skip live lanes, but an explicitly requested live lane
must fail clearly when prerequisites are missing.

### Ordinary STREAM qualification

For Java I2P, i2pd, and i2pr where current advertised SAM support permits:

1. HELLO;
2. create/import Destination;
3. NAMING lookup;
4. STREAM session create;
5. outbound CONNECT exact-byte payload exchange;
6. inbound ACCEPT exact-byte payload exchange;
7. non-silent authenticated peer Destination capture;
8. concurrent pending ACCEPT behavior where supported by negotiated syntax;
9. close/cancellation cleanup.

Java I2P plus at least one second independent router must pass payload exchange before
M002 becomes strictly closed.

### Concrete Destination identity

Replace the mock-only `"TRANSIENT" == "TRANSIENT"` proof with a concrete identity proof.

For a transient owner:

- obtain the actual router-generated public Destination or canonical hash from a
  specification-defined session/naming/result path;
- record it as the owner identity;
- prove each child observes/routes under that same identity through a router-observable or
  peer-observable mechanism.

For shared qualification, evidence must include the concrete identity/hash in the test
artifact. Merely returning the input token `TRANSIENT` is a failing test.

Do not expose private Destination material in artifacts.

### Ordinary datagram modes

Implement the M003-planned ordinary modes faithfully.

For ordinary DATAGRAM1 and RAW:

- UDP forwarding send/receive;
- no-forwarding control-socket receive where the protocol/router supports the documented
  v1/v2-compatible behavior;
- direct `DATAGRAM SEND` / `RAW SEND` where the documented behavior applies;
- FROM_PORT / TO_PORT preservation;
- RAW PROTOCOL/HEADER semantics.

For DATAGRAM2 and DATAGRAM3:

- implement only the current documented/router-supported transport modes;
- never route them through the old v1/v2-compatible SEND/RECEIVED mechanism if the
  specification excludes it;
- preserve D2 authenticated and D3 unverified source typing.

For shared children:

- do not use v1/v2-compatible direct SEND/RECEIVED on the primary or subsessions;
- use the supported datagram forwarding/data plane for that router.

### Shared-session live semantics

Run at least:

- Java I2P PRIMARY;
- Java I2P MASTER compatibility if still accepted;
- i2pd MASTER;
- i2pd PRIMARY negative/unsupported row if still rejected;
- i2pr shared rows only if its SAM 3.3 server support is implemented at execution time.

For each supported shared owner:

1. create one owner with transient or imported Destination;
2. capture concrete public identity;
3. add STREAM child;
4. add DATAGRAM1 and/or RAW child;
5. add D2/D3 children only when supported;
6. exchange STREAM bytes;
7. exchange datagram bytes;
8. prove same concrete identity/linkability domain;
9. remove one child and prove sibling still functions;
10. close owner and prove remaining children stop.

### Capability learning

A feature may become Supported only after a successful semantic operation or a
test-backed explicit router profile.

Unsupported must be distinguished from transient router/network failure. Do not mark a
feature Unsupported because a peer Destination is unreachable or a local UDP port is
blocked.

### Conformance runner

Upgrade `sam-conformance` so its feature rows are semantic:

- STREAM pass requires payload exchange;
- DATAGRAM/RAW/D2/D3 pass requires payload exchange and metadata/trust validation;
- shared pass requires child payload exchange and concrete same-Destination proof;
- teardown pass requires observed child invalidation;
- unsupported is a first-class result;
- create-only probes are labeled `create_only` and never counted as capability pass.

Structured JSON must stay schema-validated.

## 7. Ordered work packages

### WP A — interop harness and pin refresh

Freeze current spec/router pins and establish repeatable router start/config/run commands.

Acceptance:

- one command per router can reach HELLO or fails with a named provisioning error;
- artifacts are deterministic enough to compare between runs.

### WP B — ordinary STREAM live closure

Run/fix Java + i2pd + i2pr ordinary paths.

Acceptance:

- Java plus at least one second router exchange exact STREAM bytes in both directions;
- i2pr results match its actual advertised support;
- defects receive deterministic regression tests.

### WP C — ordinary datagram protocol completion

Implement and qualify UDP plus documented D1/RAW control-socket modes.

Acceptance:

- D1/RAW ordinary modes have explicit live results;
- no D2/D3/shared path incorrectly uses v1/v2-compatible commands.

### WP D — Datagram2/3 qualification

Probe each router independently.

Acceptance:

- Supported rows exchange payloads with correct trust typing;
- Unsupported rows are explicit and do not poison unrelated capability state.

### WP E — shared-session identity and payload proof

Qualify PRIMARY/MASTER semantics and same-Destination identity.

Acceptance:

- concrete public Destination/hash recorded;
- payloads traverse at least STREAM + one datagram child on Java;
- i2pd MASTER disposition recorded;
- sibling removal/owner teardown proven.

### WP F — conformance runner truthfulness

Upgrade JSON result semantics and regenerate the durable matrix.

Acceptance:

- no create-only success is represented as payload conformance;
- schema/fixtures reject misleading result states.

## 8. Failure, cancellation, restart, and contention semantics

- Router startup/provisioning failure is `not_run` only when the live lane itself cannot
  start; an explicitly requested qualification command exits non-zero.
- Protocol rejection is not retried under another shared dialect unless the first attempt
  is proven side-effect-free.
- If side-effect-free fallback cannot be proven, dialect remains caller/profile explicit.
- Live test cleanup must close owners/children/sockets even after a failed assertion.
- Datagram receive cancellation may not consume another operation's frame.
- Shared owner loss wakes child waiters and makes subsequent operations return Closed.
- Concurrent ACCEPT qualification must remain within configured operation bounds.
- No test may depend on arbitrary long sleeps when a readiness/port check is available.

## 9. Compatibility and migration

This is pre-1.0 corrective work. Wire/API changes required to match the current SAM spec
are permitted but must be documented.

Do not preserve an incorrect mock-derived behavior for source compatibility.

If a router requires a narrow syntax quirk, represent it in the compatibility layer and
record the exact affected router/version.

## 10. Required tests

### Deterministic

- ordinary STREAM connect/accept payload;
- control-socket D1 receive/send;
- control-socket RAW receive/send;
- D2/D3 exclusion from v1/v2 direct mode;
- shared child exclusion from v1/v2 direct mode;
- concrete Destination capture model;
- capability-state transitions;
- unsupported vs transient error classification;
- shared child payload routing;
- child removal/sibling survival;
- owner teardown;
- conformance JSON create-only vs payload-pass distinction.

### Live

Java I2P:
- ordinary STREAM;
- ordinary D1/RAW;
- D2/D3 probe;
- PRIMARY shared STREAM + datagram;
- MASTER compatibility row;
- same-Destination proof.

i2pd:
- ordinary STREAM;
- ordinary D1/RAW;
- D2/D3 probe;
- MASTER shared semantics;
- PRIMARY disposition;
- same-Destination proof where shared mode succeeds.

i2pr:
- execute every capability currently advertised/implemented by its selected revision;
- record unsupported/not-yet-implemented rows truthfully.

## 11. Required verification commands

The implementation must establish stable commands, but closure includes at least:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
python3 scripts/check-proto-boundary.py
python3 scripts/check-api-snapshot.py
```

Plus exact live commands for all three router targets and schema validation of every
generated conformance artifact.

## 12. Documentation updates

- reference freeze pins if refreshed;
- live-router qualification record;
- conformance observations;
- client datagram transport modes;
- PRIMARY/MASTER compatibility;
- actual unsupported feature table;
- closures 002/003/004 via additive corrective authority, not rewriting history;
- roadmap/registry.

## 13. Acceptance criteria

M005 closes only when:

1. Java I2P ordinary STREAM exchanges payloads.
2. At least one second independent router ordinary STREAM exchanges payloads.
3. i2pr's actually advertised SAM features are exercised or a specific upstream blocker
   is named.
4. ordinary DATAGRAM1/RAW UDP modes are live-qualified.
5. ordinary documented control-socket D1/RAW modes are implemented and qualified, or a
   new explicit architecture decision narrows the supported product surface with evidence.
6. D2/D3 rows are independently observed and trust semantics remain correct.
7. shared-session qualification records a concrete Destination/hash, not `TRANSIENT`.
8. Java shared STREAM + at least one datagram child exchange payloads under that identity.
9. i2pd MASTER/PRIMARY behavior is live dispositioned.
10. child removal and owner teardown are observed live for at least Java and one second
    shared-session implementation.
11. the conformance runner distinguishes create-only from semantic payload pass.
12. every live defect has a deterministic regression test.
13. M002/M003 may be promoted from conditional only to the degree this evidence actually
    closes their stated acceptance criteria.

## 14. Stop conditions

Stop and register a successor if:

- current spec and a deployed router disagree on source-authentication semantics;
- Java and i2pd require incompatible shared-session ownership rather than a narrow dialect;
- same-Destination identity cannot be observed without exposing private key material;
- a purported fallback can leave duplicate shared owners;
- D2/D3 deployed semantics materially differ from the current published proposal/spec;
- i2pr claims a feature in its support inventory that this client demonstrates is
  nonfunctional.

## 15. Closure evidence required

- exact router versions/SHAs and provisioning commands;
- exact SAM negotiated versions;
- full structured matrix;
- concrete shared Destination/hash evidence;
- payload proofs;
- control-socket datagram evidence;
- capability/unsupported rows;
- deterministic regression list;
- all commands actually executed;
- residual unsupported rows and whether they are client gaps or router limitations.

## 16. Handoff notes

Do not begin M006 by polishing mocks before M005 establishes wire truth. Any live defect
that changes public semantics must be fixed here first so M006 stabilizes the corrected
API rather than the prototype API.
