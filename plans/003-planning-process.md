# i2pr-sam Planning Process

Status: canonical.

This file adapts CodeGG's planning/closure discipline to a new protocol library.

## Baseline rule

Each implementation plan names the repository baseline it was researched against. Before
implementation starts, refresh the baseline and stop if protocol/reference evidence has
materially changed.

## Plan readiness

A plan is **ready** only when:

- every hard dependency is closed;
- required interface contracts are written;
- unresolved architecture questions are either decided by ADR or named stop conditions;
- external interoperability prerequisites are available or explicitly deferred;
- the work fits one coherent implementation pass.

## Closure rule

Code landing is not closure. A closure record must contain:

- implementation SHA;
- commands actually executed;
- protocol/reference revisions used;
- positive and negative tests;
- interoperability rows actually exercised;
- deviations/quirks found;
- residual risk and deferred work;
- successor unblock audit.

Do not report a command as passed if it was not run.

## Correctives

A discovered defect receives a new implementation plan. Prior closure records remain
historical evidence and may be superseded additively.

## Research/source discipline

Protocol work starts from normative specifications. Existing library/router source may be
inspected to understand externally observable behavior, compatibility, and test cases,
but source must not be copied into i2pr-sam.

Milestone 001 freezes exact source/spec revisions used for the initial implementation
line.

## Interoperability evidence

Unit tests against a mock bridge prove parser/state behavior, not router compatibility.
Interop claims require live/reference bridge evidence.

At minimum, milestones that claim broad compatibility distinguish Java I2P, i2pd, and
i2pr results. I2P+ may be added when it exposes a distinct deployed behavior requiring
coverage.

## Status vocabulary

- proposed
- ready
- active
- blocked
- closing
- closed
- conditionally closed
- superseded
- archived
