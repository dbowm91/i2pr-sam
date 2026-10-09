# SAM Library Milestone 018 — C ABI and Python bindings

Status: closed — see `plans/closure/sam-library/007-status.md`.

Repository baseline: `84c5727` (`main`).

Source roadmap: `plans/subsystems/sam-library-roadmap.md#007--c-abi-and-python-bindings`.

Long-term requirements: `plans/000-long-term-specification.md`, `plans/001-terminology-and-domain-model.md`.

Applicable ADRs: ADR-0001 and ADR-0002.

## 1. Objective

Add pre-1.0 C and Python bindings over the existing Rust blocking facade. Preserve the
Rust client as the only SAM implementation and keep publication/package release out of
scope.

## 2. Why this milestone is ready

M011 and M013 are closed, and M017 is merged to `main`. The blocking facade provides the
sync execution model needed by both foreign interfaces. No repository license has been
selected, so crate/Python package publication remains prohibited; local builds and source
interfaces do not require publication.

## 3. Invariants

- SAM framing, policy, and runtime ownership remain in Rust crates.
- C callers interact only through opaque handles, bounded UTF-8 inputs, explicit result
  codes, and caller-owned buffers.
- Every exported C function is panic-contained; no panic or Rust allocation crosses ABI.
- Secret Destinations and credentials are never emitted in default diagnostics.
- Python wraps the same blocking APIs, uses `ValueError` for malformed endpoints, and
  exposes a stable `SamError` exception for operational failures.
- No public package publication or license declaration is added.

## 4. Scope

In scope: a `cdylib`/`staticlib` C surface and matching Python methods for client
connect/close, name lookup, and Destination generation; checked-in C header, API
documentation, build metadata, and deterministic ABI/module smoke checks.

Out of scope: async Python APIs, full mirroring of every Rust type, crate/wheel release,
daemon integration, Windows installer work, license selection, and wire-protocol changes.

## 5. Ordered work packages

1. Define and document the opaque-handle C ABI, ownership, error mapping, bounds, and
   panic boundary.
2. Implement C client operations and a checked-in header.
3. Implement the matching PyO3 module over `i2pr-sam-blocking`.
4. Add ABI/header and Python import/loopback-mock tests, then update public API and CI
   guards as needed.

## 6. Failure and lifecycle semantics

Handles have one owning release function; null, stale, and invalid handles return an error
without dereference. Output buffers are caller-owned. Inputs are bounded and validated
before Rust calls. Blocking methods retain the existing nested-runtime refusal. Python
raises `ValueError` for malformed endpoints and `SamError` for operational failures.

## 7. Required verification

Run formatting, workspace check/test/clippy/docs, MSRV check/test, API snapshot and its
self-test, C header/ABI smoke tests, Python module tests, and the existing deterministic
conformance/guard suite. Do not claim live router evidence; M007 adds no protocol behavior.

## 8. Acceptance criteria

- C and Python can connect to a mock SAM bridge, perform lookup and Destination generation,
  and close all handles/resources.
- C ABI ownership and error behavior are documented and tested, including invalid inputs.
- Python exceptions and secret redaction are tested.
- All workspace guards and verification pass on the final implementation revision.
- Closure records exact revision, commands, results, and the successor unblock audit.

## 9. Stop conditions

Stop if safe handle ownership cannot be maintained without weakening the workspace
boundary, if a required API change breaks the reviewed Rust surface, or if distribution
work would require an unresolved license decision.

## 10. Closure evidence

See `plans/closure/sam-library/007-status.md` for implementation SHA, commands actually
run, ABI ownership/error contract, Python surface, test counts, API snapshot effect,
residual risks, and M008/M009 readiness.
