# Milestone 005 status - Live interoperability and protocol-closure corrective

Status: **conditionally closed**

Implementation revision: `2439485a63319ae8501ac3ccad789f886a4465e0`
Closure revision: this record, committed immediately after the implementation revision above
Plan: `plans/implementation/sam-library/005-foundation-live-interoperability-and-protocol-closure-corrective.md`
Predecessor: `plans/closure/sam-library/004-status.md`

## 1. What closed

The corrective work is complete and deterministic. The live payload matrix is not, and no
live row is claimed. Six of the seven acceptance areas are met by work verified on this
host; the live-lane area is not met at all, which is why the milestone is conditional rather
than closed.

## 2. Defects found and corrected

Every item below was a real protocol error, invisible while only mock bridges existed.

1. **`STREAM ACCEPT` peer identity was never consumed.** The router answers a non-silent
   accept with `STREAM STATUS RESULT=OK`, then the peer block `$destination`,
   `FROM_PORT=`, `TO_PORT=`, blank line, then payload. Milestone 004 began payload framing
   at the status line, so applications received the identity block as stream data, and
   `remote_destination()` could never return a value because the peer identity never appears
   on the status line. Fixed: `SamStream::peer()` and `remote_destination()` report the
   announced peer, `accept_with(silent)` selects `SILENT=true`, and any line that is not part
   of the block is pushed back so a router that omits the terminator cannot corrupt payload.
2. **Ordinary DATAGRAM1/RAW control-socket modes were absent.** `DATAGRAM SEND`/`RAW SEND`
   and `DATAGRAM RECEIVED`/`RAW RECEIVED` are now implemented, including `SIZE`-delimited raw
   bodies, reply demultiplexing on one socket, and a bounded delivery queue that drops the
   newest datagram and counts the drop rather than growing.
3. **RAW forwarding metadata was unconditional.** A forwarded RAW datagram carries
   FROM_PORT/TO_PORT/PROTOCOL only with `HEADER=true`. Without it the packet is bare payload;
   the decoder invented a header. Fixed, and `HEADER` is now requested by default only from
   SAM 3.2.
4. **Shared identity compared the request token.** Shared-session identity is now a concrete
   `SessionIdentity` resolved through `NAMING LOOKUP NAME=ME`, with a canonical SHA-256 hash.
   A shared session that cannot resolve a concrete Destination fails with
   `SamError::IdentityUnavailable`; `TRANSIENT` is never returned as identity. Every
   subsession reports the owner's identity.
5. **Style rejections were not capability verdicts.** `INVALID_STYLE` now yields
   `SamError::Unsupported` and records `Support::Unsupported`, while `CANT_REACH_PEER`,
   `KEY_NOT_FOUND`, and `TIMEOUT` stay transient.
6. **Conformance rows overstated results.** The runner now records
   `pass|unsupported|fail|not_run|create_only` with payload evidence; `create_only` never
   counts as a capability pass, and `scripts/check-conformance-artifact.py` rejects an
   artifact that claims a pass without bytes moving in both directions.
7. **Reserved protocol numbers were not enforced on RAW subsessions.** 6, 17, 19, and 20 are
   now rejected for `STYLE=RAW` per the specification's explicit exclusion.

## 3. Evidence

Commands executed on this host, with observed results:

| Command | Result |
|---|---|
| `cargo test --locked -p i2pr-sam-proto` | pass |
| `cargo test --locked -p i2pr-sam` | pass, including the new deterministic mock suite |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo +1.89 test --workspace` (MSRV) | pass |
| `python3 scripts/interop/udp_egress_probe.py` | exit 1, `i2p_router_udp_capable=false` |
| `python3 scripts/interop/qualify.py --all` | exit 3, three `not_run` artifacts |
| `python3 scripts/check-conformance-artifact.py <each artifact>` | valid |
| `python3 scripts/check-conformance-artifact.py --self-test` | 13/13 cases behaved as expected |

Router pins re-verified on 2026-10-07 with `git ls-remote`, unchanged from the freeze:
Java I2P `a629ec7c9c675dd252d005fb881efd92e8e6ba27`, i2pd
`d147bb0fd6789c75dc1c4d70c4f79b151a552d53`, i2pr
`4f4e98f5a0241355af5099c73065088dede1b1de`.

## 4. Acceptance criteria

| # | Criterion | Result |
|---|---|---|
| 1 | router-enabled qualification harness under `scripts/` and live-capable tests | met - `scripts/interop/` provisions, probes, runs, validates, merges; exit `3` names the blocker |
| 2 | live Java I2P STREAM/DATAGRAM/RAW rows | **not met - no router and no SSU-capable UDP egress on this host** |
| 3 | D2 and D3 qualification with no capability inferred from version | met - each style is probed independently and recorded per row; transport exclusion enforced before any command |
| 4 | ordinary v1/v2-compatible control-socket DATAGRAM/RAW paths | met - implemented, bounded, regression-tested |
| 5 | no D2/D3/shared path uses v1/v2-compatible commands | met - `SessionStyle::supports_control_socket_datagram` is the single exclusion point; shared children have no such API |
| 6 | inbound STREAM payload exchange | implemented and deterministically tested; live row not run |
| 7 | non-silent peer Destination capture on accept | met - implemented and deterministically tested; live row not run |
| 8 | shared rows record concrete Destination identity plus peer-observable evidence | met - `SessionIdentity` plus per-child identity proof in the runner; live row not run |
| 9 | conformance runner demands payload evidence | met - schema and validator reject a pass without evidence |
| 10 | no unexplained router state | met - unsupported and transient failures are separate classes with distinct diagnostics |
| 11 | harness produces comparable per-router matrix | met - `artifacts/interop/interop-matrix.csv` |
| 12 | live lane that cannot start is named, not implied | met - exit 3, `not_run` rows, `capability_passes=0` |
| 13 | M002/M003 promotion only to the degree evidence supports | **no promotion** - no live row passed, so their conditional authority stands |

## 5. Why this is conditional rather than closed

Criteria 2 and the live half of 6-8 require a router that can reach the I2P network. This
host has no router binary, no container runtime access, and UDP egress that answers only on
port 53. I2P peers over SSU on random high UDP ports, so a router here could reseed and
then never build a tunnel. That is an environment prerequisite failure, not a protocol
result, and the harness records it as such.

Residual work is bounded and registered as
`plans/implementation/sam-library/011-live-router-payload-qualification.md`. The harness,
runner, schema, and validator that milestone 011 needs already exist and have been executed
in their blocked path.

## 6. Successor audit

| Milestone | Decision |
|---|---|
| M006 | unblocked to execute, not to close strictly. Its acceptance requires M005's live wire truth for API stabilisation; that authority is still conditional, so M006 closes conditionally with the same named residual. |
| M007 bindings | remains blocked on M006 strict closure |
| M008 i2pr adapter | remains blocked on M006 plus a stable merged `i2pr-service-tunnels` revision |
| M011 live payload qualification | registered as the successor for this milestone's residual |

## 7. Honest limits

- No live interoperability claim exists for this client.
- Deterministic mocks encode the specification as read on 2026-10-07; a router that
  disagrees will surface as a finding in milestone 011, not as a defect in this record.
- `create_only` results prove a router accepted a session, nothing more.