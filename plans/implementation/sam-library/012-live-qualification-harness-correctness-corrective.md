# 012 — Live qualification harness correctness corrective

Class: corrective infrastructure + invariant.

Status: **ready**.

Repository baseline: `15f93089040691a1354d9c3b24d31a586ed655af`.

Corrects:

- `plans/implementation/sam-library/011-live-router-payload-qualification.md`;
- `scripts/interop/qualify.py`;
- `scripts/interop/udp_egress_probe.py`;
- `.github/workflows/live-interop.yml`;
- repository hygiene around generated Python artifacts.

## 1. Objective

Make milestone 011's qualification machinery actually capable of producing the evidence
its plan requires, and remove provisioning conclusions that the current probes cannot
justify.

At the current baseline M011 is **not** ready merely by moving to a router-enabled host:

1. `runner_command()` returns its argv before the `peer_endpoint` branch, so
   `--peer-endpoint` can never reach `sam-conformance`;
2. the manual workflow explicitly refuses to pass its peer-endpoint input even though
   `qualify.py` now accepts `--peer-endpoint`;
3. the workflow runs on a GitHub-hosted `ubuntu-latest` machine, so loopback SAM
   endpoints refer to the ephemeral Actions VM, not to a user's pre-provisioned router
   host;
4. the UDP probe interprets silence from arbitrary `1.1.1.1:443` and
   `1.1.1.1:34567` as evidence that those outbound ports are blocked. An endpoint with
   no cooperating UDP service produces the same observation on an unrestricted network;
5. `scripts/interop/__pycache__/qualify.cpython-312.pyc` is tracked while
   `.gitignore` ignores only `/target/`.

M012 makes these facts impossible to regress before M011 is returned to
`ready on provisioning`.

## 2. Why this milestone is ready

All defects are deterministic repository defects visible without a live router. No router
provisioning is needed to correct or verify them.

M012 therefore precedes M011 but does not depend on it.

## 3. Current evidence

### Peer endpoint

`qualify.py` exposes `--peer-endpoint`, and `qualify_one()` passes
`args.peer_endpoint` into `run_runner()`. However `runner_command()` is currently:

```python
return [ ... ]
if peer_endpoint:
    argv.extend(["--peer-endpoint", peer_endpoint])
return argv
```

The extension is unreachable.

The workflow has a `peer_endpoint` dispatch input but contains a step named
`Note unplumbed peer_endpoint input` and invokes `qualify.py` without the input.

### Live workflow topology

`live-interop.yml` uses `runs-on: ubuntu-latest`. A SAM endpoint such as
`127.0.0.1:7656` therefore targets the hosted runner itself. The workflow neither
installs nor starts Java I2P, i2pd, or i2pr, so a user's locally running routers cannot be
qualified through that workflow.

### UDP probe

The probe does one request/reply attempt against:

- `1.1.1.1:53` with DNS;
- `1.1.1.1:443` with an improvised QUIC-shaped datagram;
- `1.1.1.1:34567` with the same arbitrary datagram.

Only the DNS endpoint is known to provide a matching service. A timeout on the other two
does **not** distinguish an egress filter from a reachable host that simply does not
reply. The current `i2p_router_udp_capable=false` conclusion therefore overstates the
evidence.

### Generated artifacts

The branch tracks both:

- `scripts/interop/__pycache__/`
- `scripts/interop/__pycache__/qualify.cpython-312.pyc`

and `.gitignore` has no Python cache pattern.

## 4. Invariants

- A qualification prerequisite may fail closed only when the probe can distinguish the
  prerequisite from an uncooperative external endpoint.
- Unknown network reachability is represented as **unknown/advisory**, not `blocked`.
- A peer-payload lane cannot be reported runnable unless the peer endpoint reaches the
  conformance binary's argv.
- A GitHub-hosted runner may not imply access to localhost services on another machine.
- A self-hosted live workflow, if retained, must identify its execution topology
  explicitly.
- `pass` remains reserved for semantic payload evidence.
- Environment inability remains distinct from protocol `unsupported` and protocol
  `fail`.
- Generated Python bytecode/cache directories are never tracked.

## 5. Scope

### In scope

- repair peer-endpoint argv construction;
- pass peer endpoint end-to-end through local harness and any supported workflow;
- add deterministic command-construction tests;
- replace the invalid high-port UDP hard gate with a sound readiness contract;
- correct live-workflow topology;
- add harness-level tests for provisioning classifications and exit codes;
- remove tracked Python cache artifacts and extend `.gitignore`;
- reconcile M011/docs/closure prose that currently cites unsupported UDP conclusions.

### Out of scope

- executing M011 live payload rows;
- changing SAM wire behavior;
- adding C/Python bindings;
- changing router implementations;
- adding a permanent public external UDP dependency to normal CI.

## 6. Required production changes

### A. Peer endpoint plumbing

Refactor `runner_command()` to construct a mutable argv, conditionally append
`--peer-endpoint`, then return it.

Add deterministic tests proving:

- absent peer endpoint -> no flag;
- provided endpoint -> exactly one flag and value;
- endpoint survives `qualify_one -> run_runner -> runner_command`;
- no stale duplicate or positional ambiguity appears.

The workflow must pass `--peer-endpoint "$PEER_ENDPOINT"` when non-empty. Remove the
stale warning that says `qualify.py` has no such flag.

### B. Sound UDP readiness semantics

Do not use non-response from an arbitrary public IP/port as proof that outbound UDP is
blocked.

Choose one of these evidence-equivalent approaches:

1. **preferred:** make the UDP preflight advisory by default and let a real provisioned
   router's network/tunnel readiness be the hard prerequisite;
2. support an explicitly configured cooperative UDP echo/STUN-style endpoint whose
   protocol is expected to reply, and only then allow the probe to produce a hard
   reachable/unreachable verdict;
3. use another independently verifiable OS/network mechanism that actually distinguishes
   policy rejection from remote silence.

The result model must be at least tri-state:

- `reachable`;
- `unreachable` only with positive evidence;
- `unknown`.

`unknown` must not synthesize `provisioning_udp_egress_blocked`.

The existing DNS result may prove that **some UDP request/reply works**. It does not prove
that only port 53 works.

### C. Router readiness

A provisioned-router lane should prefer direct evidence:

- expected SAM bridge answers HELLO;
- router has reached a usable network state appropriate for the lane;
- when two routers are required, both bridges are reachable and the conformance runner is
  given both endpoints.

Do not require an unrelated public UDP host to answer before attempting a known-good local
router.

### D. Workflow topology

The workflow must choose one truthful model.

**Model 1 — self-hosted interop runner (preferred):**

- `runs-on` targets a dedicated self-hosted label;
- documentation states that routers/endpoints must be provisioned on or routable from that
  host;
- workflow input endpoints are meaningful in that network namespace.

**Model 2 — GitHub-hosted and self-provisioning:**

- the workflow installs/starts the selected pinned router(s) itself;
- readiness is proven before qualification;
- no local-machine loopback assumption exists.

If neither is currently available, disable/remove the hosted workflow as an evidence path
and document M011 as a local/self-hosted command. A workflow that can only return
`not_run` on its own topology is not a qualification lane.

### E. Harness regression tests

Add a Python test suite covering at least:

- peer endpoint argv;
- peer endpoint workflow invocation/static contract;
- known bridge unavailable;
- binary unavailable;
- advisory UDP unknown;
- cooperative UDP positive and negative fixtures without public-network dependence;
- artifact synthesis;
- exit 0/1/2/3 precedence;
- invalid artifact outranking blocked lanes.

Use loopback test servers or injected probes; ordinary tests must not depend on the public
Internet.

### F. Repository hygiene

Remove tracked `__pycache__` and `*.pyc` artifacts.

Extend `.gitignore` with at least:

```gitignore
__pycache__/
*.py[cod]
```

Add a cheap CI/static guard that fails if generated Python bytecode/cache entries become
tracked again.

## 7. Ordered work packages

### WP A — peer endpoint repair and tests

Exit: a unit/integration test proves `--peer-endpoint` reaches the conformance argv.

### WP B — UDP/readiness model correction

Exit: arbitrary UDP silence is classified unknown, not blocked; any hard negative verdict
has cooperative/positive evidence.

### WP C — live workflow topology correction

Exit: workflow either runs where the routers are reachable, provisions them itself, or is
explicitly removed as an evidence lane.

### WP D — harness classification tests

Exit: deterministic tests cover command construction, readiness, result synthesis, and
exit-code precedence.

### WP E — generated-artifact cleanup

Exit: no tracked Python caches; CI guard and ignore patterns prevent recurrence.

### WP F — M011 reconciliation

Exit: M011 no longer says the only remaining requirement is a router-enabled host unless
that is actually true after this implementation.

## 8. Failure, cancellation, restart, and contention semantics

This plan changes harness/process logic, not SAM session ownership.

- child router processes started by a self-provisioning workflow must have bounded startup
  and teardown;
- workflow cancellation must terminate started router processes;
- stale artifacts from previous runs are removed before a lane starts;
- peer endpoint omission must deterministically produce structural/not-run payload rows,
  never silently downgrade a requested peer lane;
- probe timeouts are bounded and their ambiguity preserved.

## 9. Compatibility and migration

No public Rust API change is required.

The interop CLI already documents `--peer-endpoint`; fixing it is a bug fix. Changes to
UDP result fields may require updating artifact/harness documentation but should not alter
SAM conformance schema unless those fields are part of the schema.

## 10. Required tests

- `runner_command` without peer endpoint;
- `runner_command` with peer endpoint;
- workflow/static test proving peer endpoint is passed;
- advisory/unknown UDP case;
- cooperative UDP success;
- cooperative UDP explicit negative if a sound negative mechanism is implemented;
- binary missing;
- bridge missing;
- bridge HELLO success;
- result precedence;
- generated Python artifact guard;
- all existing conformance validator self-tests.

## 11. Verification commands

At minimum:

```bash
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tests -p 'test_*.py'
python3 scripts/check-conformance-artifact.py --self-test
python3 scripts/check-proto-boundary.py --self-test
python3 scripts/check-api-snapshot.py --self-test
git ls-files | grep -E '(^|/)(__pycache__/|.*\.py[co]$)' && exit 1 || true
```

If workflow-validation tooling exists, validate all workflow YAML. Otherwise add an
equivalent static test for the live workflow's peer-endpoint contract.

## 12. Documentation updates

- `scripts/interop/README.md`;
- `specs/live-router-qualification.md`;
- M011 implementation plan;
- README live qualification instructions;
- registry/roadmap;
- any workflow operator documentation.

Historical M005/M006 closure records remain immutable; add an additive correction note or
M012 closure reference in current planning instead of rewriting what they observed.

## 13. Acceptance criteria

M012 closes only when:

1. `--peer-endpoint` reaches `sam-conformance` and has a deterministic regression test.
2. The manual/self-hosted workflow passes the peer endpoint or is no longer presented as
   a usable live evidence path.
3. Workflow topology can actually reach/provision the configured routers.
4. Arbitrary non-response from `1.1.1.1:34567` is no longer used as proof of blocked
   high-port UDP.
5. UDP readiness has truthful reachable/unreachable/unknown semantics.
6. A provisioned SAM bridge can be attempted even when advisory UDP status is unknown.
7. Harness exit-code/result classification has deterministic tests.
8. No Python cache/bytecode file is tracked.
9. Generated Python artifacts are ignored/guarded.
10. M011 documentation is reconciled so its remaining blocker is truthful.
11. Normal hosted CI remains green.

## 14. Stop conditions

Stop and register a successor if:

- a trustworthy hard UDP-negative result would require a maintained external service not
  owned or explicitly selected by the project;
- GitHub-hosted Actions cannot safely provision the routers and no self-hosted runner is
  available — in that case remove hosted live qualification rather than fake it;
- peer-to-peer payload qualification requires a topology different from the current
  two-bridge runner contract.

## 15. Closure evidence required

- before/after command argv proof;
- peer-endpoint harness test;
- workflow topology decision;
- UDP probe semantic decision and tests;
- tracked-artifact cleanup proof;
- harness unit-test counts;
- current CI run at the exact closure head;
- updated M011 dependency/readiness state.

## 16. Handoff notes

M011 remains blocked until M012 closes. A router-enabled host should not be asked to run a
lane whose peer endpoint can still be dropped or whose prerequisite probe can falsely
classify ordinary UDP silence as an egress policy.
