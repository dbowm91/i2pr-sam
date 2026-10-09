# 015 — Conformance DATAGRAM2 runner dispatch corrective

Class: corrective qualification invariant.

Status: **closed**; see `plans/closure/sam-library/015-status.md`.

## Objective

Make every SAM datagram family produce a conformance row without panicking before the
row can report support, unsupported behavior, a protocol failure, or an environmental
not-run result.

## Finding

M011's first full run against pinned i2pd reached the DATAGRAM2 row and panicked because
`datagram_row()` marked every style except ordinary DATAGRAM and RAW as unreachable.
The plan explicitly includes DATAGRAM2 and DATAGRAM3, so the runner could not reach or
record those rows.

## Scope

- map DATAGRAM, RAW, DATAGRAM2, and DATAGRAM3 styles to their matching artifact features;
- use the mapping in both peer-present and peer-absent row construction;
- add a focused regression test for every style mapping;
- rerun the runner tests and a live full-plan attempt against the pinned i2pd build.

## Acceptance criteria

1. Constructing the DATAGRAM2 or DATAGRAM3 row never enters an unreachable branch.
2. Each style emits its own feature name in the artifact.
3. A live full-plan attempt exits through documented result handling and writes a valid
   artifact, even if the router reports unsupported capabilities or the environment
   cannot build tunnels.

## Verification

```bash
cargo test --locked -p i2pr-sam --bin sam-conformance
python3 scripts/interop/qualify.py --router i2pd --endpoint 127.0.0.1:19856 \
  --peer-endpoint 127.0.0.1:7656 --plan full --skip-udp-probe
```

## Closure evidence

Record implementation and closure revisions, test results, and the full-plan artifact in
`plans/closure/sam-library/015-status.md`. This corrective does not satisfy M011's
multi-router payload acceptance criteria by itself.
