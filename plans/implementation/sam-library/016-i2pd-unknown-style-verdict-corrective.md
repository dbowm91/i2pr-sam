# 016 — i2pd unknown-style verdict corrective

Class: corrective interoperability invariant.

Status: **ready**.

## Objective

Represent i2pd's explicit `I2P_ERROR MESSAGE="Unknown STYLE"` reply as an unsupported
capability verdict throughout the client and qualification artifact.

## Finding

The pinned i2pd router reports an unsupported shared dialect as `I2P_ERROR` with the
message `Unknown STYLE`. The client recognized only the standard `INVALID_STYLE` and
`INVALID_ID` result codes, so M011's full matrix emitted a `fail` for i2pd's documented
unsupported PRIMARY dialect even though i2pd retains MASTER. This makes the matrix's
failure state inaccurate and prevents it from distinguishing the router's explicit
capability verdict from an implementation failure.

## Scope

- recognize only the exact `Unknown STYLE` message when paired with `I2P_ERROR`;
- update client capability learning and error classification;
- add protocol-level and SAM-client regression tests;
- rerun the workspace tests and M011's full i2pd matrix.

Other `I2P_ERROR` replies remain rejected errors. No generic server error is reclassified.

## Acceptance criteria

1. Standard `INVALID_STYLE` and `INVALID_ID` remain unsupported verdicts.
2. `I2P_ERROR MESSAGE="Unknown STYLE"` becomes `SamError::Unsupported` and updates the
   requested capability to unsupported.
3. Other `I2P_ERROR` messages remain ordinary rejections.
4. The live full-plan artifact records PRIMARY as unsupported and does not count it as a
   router conformance failure.

## Verification

```bash
cargo test --locked --workspace --all-targets
python3 scripts/interop/qualify.py --router i2pd --endpoint 127.0.0.1:19856 \
  --peer-endpoint 127.0.0.1:7656 --plan full --skip-udp-probe
```

## Closure evidence

Record the implementation and closure revisions, tests, and full-plan artifact at
`plans/closure/sam-library/016-status.md`. This verdict correction does not replace M011's
required successful payload exchanges.
