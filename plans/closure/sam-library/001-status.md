# SAM Library M001 closure

Status: **closed**.

Implementation SHA: `956238fcced742836833befaf6e748e97f435001`.

## Evidence

- Reference freeze: `specs/references/sam-v3-reference-freeze.md`, retrieved 2026-10-07.
- Pinned deployed references: Java I2P `a629ec7c9c675dd252d005fb881efd92e8e6ba27`,
  i2pd `d147bb0fd6789c75dc1c4d70c4f79b151a552d53`, and i2pr
  `4f4e98f5a0241355af5099c73065088dede1b1de`. These are remote HEAD revisions, not
  locally executed router versions.
- SAM protocol text is runtime-neutral. `cargo tree -p i2pr-sam-proto` lists only the
  protocol crate. `scripts/check-proto-boundary.py` passed and its positive control
  detected a synthetic Tokio dependency.
- The codec has named ceilings: 16 KiB line, 128 tokens, 128-byte key, 8 KiB value, and
  8 KiB Destination/private-key text. SAM does not specify a line maximum; 16 KiB provides
  room for bounded extension fields while the 8 KiB value limit exceeds the documented
  884-plus-character private key form. Values are checked before retaining them.
- Typed command/reply/result, version, capability, style, Destination, secret,
  port/protocol, session ID, and phase types are implemented. Unknown result text is
  preserved. Secret-bearing parsed field values and private Destination Debug output are
  redacted.

## Commands executed

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo check --locked --workspace --all-targets` — passed.
- `rtk cargo test --locked --workspace --all-targets` — passed; 18 workspace tests at
  final verification.
- `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` —
  passed.
- `rtk env RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` — passed.
- `rtk cargo tree -p i2pr-sam-proto` — protocol crate only.
- `rtk python3 scripts/check-proto-boundary.py` — passed, including positive control.
- `rtk cargo +1.89 check --locked --workspace --all-targets` — passed.
- `rtk cargo +1.89 test --locked --workspace --all-targets` — passed.
- `rtk cargo +1.89 clippy --locked --workspace --all-targets --all-features -- -D warnings`
  — passed.

Codec evidence includes quoted-value round trips, generated legal-value round trips,
max/max+1 boundaries, duplicate rejection, invalid line endings, 4,096 deterministic
bounded arbitrary-byte parser inputs, state-transition legality, capability/version
separation, and redaction checks.

## Deviations and residual risk

The property smoke test is deterministic generation in the unit suite rather than an
external fuzzing campaign. No implementation source was copied. Router behavior is not
asserted by M001 and is qualified by later milestones.

## Successor audit

M002's protocol dependency is satisfied and M002 was implemented against this exact
substrate. M002's live-router acceptance remains an external qualification condition and
is recorded in its closure; it did not block implementing independent later work.
