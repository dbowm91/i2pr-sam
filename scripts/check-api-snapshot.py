#!/usr/bin/env python3
"""Guard the frozen public Rust surface with a reviewable declaration snapshot.

Why a snapshot and not a semver tool: M006 freezes the pre-1.0 surface before
C/Python bindings amplify every mistake in it, and the artifact that has to
survive code review is a line-oriented, sorted list of declarations. A regex
inventory cannot prove semantic compatibility, so the plan forbids claiming it.
This guard therefore does two honest things and nothing more:

1. it records public declaration *text* per crate and reports drift against the
   checked-in baseline, and
2. `--self-test` proves the detector fires on known synthetic breakages, which
   is the strongest claim a text inventory is entitled to make.

What this inventory CANNOT observe (deliberately not claimed):

* macro-generated items. `macro_rules!` output, `derive`-generated inherent
  methods and anything else produced by expansion never appear as source text.
* `cfg` evaluation. Only the branch present in the file is read; the guard
  cannot tell which gated item belongs to the supported surface, and a change
  that only moves an item between `cfg` arms is invisible.
* re-export reachability. `pub use` lines are recorded as text, but visibility
  through private modules, glob re-exports, private re-export chains and name
  ambiguity are not resolved, so a re-export that stops resolving is missed.
* generic bounds, `where` clauses and associated-type bounds. They are recorded
  only as flattened header text; the guard does not decide whether a bound
  change is breaking.
* trait-provided behaviour. Only the trait declaration is recorded, never
  default method bodies and never `impl` blocks, so added or changed trait
  methods and `impl` changes are missed entirely.
* attributes. Attribute lines are skipped, so `#[non_exhaustive]` and its
  downstream construction/matching semantics, `derive` lists, `cfg` attributes
  and `doc` attributes are untracked.
* restricted visibility. Only `pub` items are inventoried. `pub(crate)` and
  `pub(super)` items are invisible and module privacy is not modelled; demoting
  `pub fn` to `fn` is therefore observed only as the entry disappearing, which
  is why the self-test includes that mutation explicitly.
* precise brace matching. Struct/enum bodies are delimited by counting braces,
  so a brace inside a string or char literal can end a body early.
* struct-variant payload. An enum variant with named fields is recorded by
  variant name only; its fields are not expanded.
* one-row-per-line assumptions. Field and variant rows assume one member per
  line, which rustfmt produces but which is not enforced by the guard.
* semantic change behind identical text. An `impl` body, a constant's value, a
  generic argument's meaning, a changed type alias target's behaviour, or a
  behaviour change behind an unchanged signature are all invisible.

This is not rustdoc JSON, not a type checker, and not semver-complete.

Exit codes (unchanged for callers, closures and CI):

  0  snapshot matches the current inventory
  1  drift, including a snapshot still written in the older, coarser format
  2  the guard's own positive control failed, i.e. the guard is broken

Usage:

  python3 scripts/check-api-snapshot.py               compare against the baseline
  python3 scripts/check-api-snapshot.py --write       regenerate the baseline
  python3 scripts/check-api-snapshot.py --self-test   prove the detector fires

`--self-test` copies the synthetic fixtures in `scripts/guards/mutations/api/`
into a `tempfile.mkdtemp()` sandbox and runs the ordinary inventory and
comparison functions over the copies; it never reads `crates/` or
`api/public-types.txt`, and never writes a repository file.
"""
from __future__ import annotations

import argparse
from collections.abc import Iterable, Sequence
from pathlib import Path
import re
import shutil
import sys
import tempfile

TRACKED_FILES: tuple[Path, ...] = (
    Path("crates/i2pr-sam/src/lib.rs"),
    Path("crates/i2pr-sam-proto/src/lib.rs"),
    Path("crates/i2pr-sam-blocking/src/lib.rs"),
)
SNAPSHOT_PATH = Path("api/public-types.txt")
FIXTURE_DIR = Path(__file__).resolve().parent / "guards" / "mutations" / "api"
BASE_FIXTURE = "base.rs"

# `pub(crate)`/`pub(super)` items are intentionally not matched: restricted
# visibility is neither tracked nor modelled (see the module docstring).
ITEM = re.compile(
    r"^[ \t]*pub[ \t]+(?:(?:async|unsafe|extern|const)[ \t]+)*"
    r"(?P<kind>fn|struct|enum|trait|type|const|static|use|mod)\b"
)
TYPE_NAME = re.compile(r"^pub[ \t]+(?:struct|enum|trait|type)[ \t]+([A-Za-z_]\w*)")
FIELD = re.compile(r"^pub(?:\([^)]*\))?[ \t]+([A-Za-z_]\w*)[ \t]*:[ \t]*(.+)$")
VARIANT_NAME = re.compile(r"^(?:pub[ \t]+)?([A-Za-z_]\w*)\b(.*)$")
ATTRIBUTE = re.compile(r"^(#\[|///|//!|//|/\*|\*)")
# Row shapes that only the current, richer inventory emits versus the older
# coarser one; used to tell an operator that the stored baseline predates this
# format instead of silently reporting an unexplained diff.
COARSE_ROW = re.compile(r" (?:field|variant) pub (?:struct|enum|trait)\b")
TYPED_FIELD_ROW = re.compile(r" field [A-Za-z_]\w*: ")

# Positive control for the detector itself: a synthetic source exercising every
# row shape the inventory produces. A clean run must never be the result of a
# silently broken parser.
POSITIVE_CONTROL_SOURCE = """//! positive control
pub struct Control {
    pub key: String,
}
pub enum ControlKind {
    Unit,
    Payload(u8),
}
pub use crate::Control as Alias;
pub fn control(value: &str) -> Result<Control, ControlKind> {
    let _ = value;
    Err(ControlKind::Unit)
}
"""

MUTATIONS: tuple[tuple[str, str], ...] = (
    ("removed public item", "removed_item.rs"),
    ("changed parameter type", "changed_parameter_type.rs"),
    ("added parameter", "added_parameter.rs"),
    ("enum variant added", "variant_added.rs"),
    ("enum variant removed", "variant_removed.rs"),
    ("struct field added", "field_added.rs"),
    ("struct field removed", "field_removed.rs"),
    ("pub fn demoted to private fn", "visibility_dropped.rs"),
)
NEGATIVE_CONTROL = ("unchanged source (negative control)", BASE_FIXTURE)


def canonical(text: str) -> str:
    """Collapse a declaration into one stable, line-oriented review row.

    rustfmt wraps signatures across lines and leaves a trailing comma in a
    wrapped parameter list, so whitespace and that one comma are normalized;
    every other token is preserved so a source edit still shows up as a diff.
    """
    line = " ".join(text.split())
    line = re.sub(r"[ \t]*,[ \t]*", ", ", line)
    line = re.sub(r",\s*\)", ")", line)  # rustfmt's trailing parameter comma
    line = re.sub(r"\([ \t]+", "(", line)
    line = re.sub(r"[ \t]+\)", ")", line)
    line = re.sub(r"\{[ \t]+", "{", line)
    line = re.sub(r"[ \t]+\}", "}", line)
    line = re.sub(r"[ \t]+\]", "]", line)
    return re.sub(r"\[([A-Za-z_])", r"[\1", line).strip()


def declaration(lines: list[str], start: int, *, stops_at_brace: bool = True) -> tuple[str, int]:
    """Return the canonical header starting at `lines[start]` and its last line.

    The header runs through the line that closes its parameter list and opens a
    body, or through the terminating semicolon. `use` rows set
    `stops_at_brace=False` because a re-export tree opens a brace group that is
    part of the header rather than a body.
    """
    parts: list[str] = []
    parens = brackets = 0
    end = start
    for index in range(start, len(lines)):
        line = lines[index]
        parts.append(line)
        parens += line.count("(") - line.count(")")
        brackets += line.count("[") - line.count("]")
        end = index
        closed = parens <= 0 and brackets <= 0
        opens_body = "{" in line and stops_at_brace
        if closed and (opens_body or ";" in line):
            break
    return canonical(" ".join(parts)), end


def block_body(lines: list[str], start: int) -> list[str]:
    """Return the stripped lines inside the block whose header ends at `start`."""
    body: list[str] = []
    depth = 0
    for raw in lines[start:]:
        stripped = raw.strip()
        if depth == 0:
            if "{" not in stripped:
                continue
            depth = raw.count("{") - raw.count("}")
            trailing = stripped.split("{", 1)[1].strip()
            if depth <= 0:
                return body
            if trailing and not trailing.startswith("}"):
                body.append(trailing)
            continue
        depth += raw.count("{") - raw.count("}")
        if depth <= 0:
            return body
        if stripped:
            body.append(canonical(stripped))
    return body


def field_type(text: str) -> str:
    """Return a field's type text without its default value or trailing comma."""
    return re.sub(r"\s*=[^=]*$", "", text).strip().rstrip(",").strip()


def variant_row(line: str) -> str | None:
    """Return a `Name` or `Name(payload)` row for an enum variant line."""
    if ATTRIBUTE.match(line):
        return None
    match = VARIANT_NAME.match(line)
    if match is None:
        return None
    name = match.group(1)
    rest = match.group(2).strip().rstrip(",").strip()
    if rest.startswith("{"):
        return name  # struct variant: named fields are not expanded
    if rest.startswith("("):
        close = rest.rfind(")")
        return f"{name}({rest[1:close] if close > 0 else rest[1:]})"
    if not rest or rest.startswith("="):
        return name  # unit variant, possibly with a discriminant
    return None


def member_rows(lines: list[str], end: int, kind: str, name: str) -> list[str]:
    """Return the field or variant rows for a struct or enum declaration."""
    rows: list[str] = []
    for line in block_body(lines, end):
        if ATTRIBUTE.match(line):
            continue
        if kind == "struct":
            field = FIELD.match(line)
            if field is not None:
                rows.append(f"pub struct {name} field {field.group(1)}: {field_type(field.group(2))}")
            continue
        variant = variant_row(line)
        if variant is not None:
            rows.append(f"pub enum {name} variant {variant}")
    return rows


def inventory_source(text: str, module: str) -> list[str]:
    """Return the sorted inventory rows for one Rust source text.

    Pure function, shared by the repository run and the self-test so the
    mutation harness exercises the production detector.
    """
    lines = text.splitlines()
    entries: set[str] = set()
    index = 0
    while index < len(lines):
        item = ITEM.match(lines[index])
        if item is None:
            index += 1
            continue
        header, end = declaration(lines, index, stops_at_brace=item.group("kind") != "use")
        if header.endswith("{"):
            header = header[:-1].strip()
        entries.add(f"{module}::{header}")
        kind = item.group("kind")
        if kind in ("struct", "enum"):
            name_match = TYPE_NAME.match(header)
            if name_match is not None:
                for row in member_rows(lines, end, kind, name_match.group(1)):
                    entries.add(f"{module}::{row}")
        index = end + 1
    return sorted(entries)


def inventory_paths(paths: Iterable[Path]) -> list[str]:
    """Return the sorted inventory rows for the given source files."""
    entries: set[str] = set()
    for path in paths:
        entries.update(inventory_source(path.read_text(encoding="utf-8"), path.as_posix()))
    return sorted(entries)


def render(rows: Iterable[str]) -> str:
    """Render inventory rows as the sorted, newline-terminated snapshot text."""
    return "\n".join(sorted(rows)) + "\n"


def drift(expected: str, current: str) -> tuple[list[str], list[str]]:
    """Return the rows the snapshot lost and the rows the source gained."""
    expected_rows = set(expected.splitlines())
    current_rows = set(current.splitlines())
    return sorted(expected_rows - current_rows), sorted(current_rows - expected_rows)


def snapshot_format(snapshot_text: str) -> str:
    """Name the inventory format a stored snapshot was written with.

    The older, coarser format recorded a struct/enum header as one of its own
    `field`/`variant` rows and never recorded field types, so its rows are
    recognisable and an operator can be told why the diff is large.
    """
    if COARSE_ROW.search(snapshot_text):
        return "coarse-v1"
    if TYPED_FIELD_ROW.search(snapshot_text):
        return "typed-v2"
    return "unrecognized"


def guard_verdict(expected: str, current: str) -> tuple[int, list[str]]:
    """Return the guard's exit code and messages for one comparison.

    Single decision point for both the repository run and every self-test case.
    """
    removed, added = drift(expected, current)
    if not removed and not added:
        return 0, []
    messages = [f"public API drift: {len(removed)} snapshot rows removed, {len(added)} added"]
    stored_format = snapshot_format(expected)
    if stored_format != "typed-v2":
        messages.append(
            f"note: the stored snapshot is in the {stored_format} format; this guard now records "
            "field types, enum payloads, full signatures and re-export rows"
        )
        messages.append(
            "action: review the rows above, then regenerate with "
            "`python3 scripts/check-api-snapshot.py --write` "
            "(expected once after the M006 guard upgrade, not a code defect)"
        )
    for row in (added[:5] + removed[:5]):
        messages.append(f"  {row}")
    return 1, messages


def positive_control_passes() -> bool:
    """Confirm the inventory still produces every row shape it is meant to."""
    rows = set(inventory_source(POSITIVE_CONTROL_SOURCE, "control"))
    required = (
        "control::pub struct Control",
        "control::pub struct Control field key: String",
        "control::pub enum ControlKind",
        "control::pub enum ControlKind variant Unit",
        "control::pub enum ControlKind variant Payload(u8)",
        "control::pub use crate::Control as Alias;",
        "control::pub fn control(value: &str) -> Result<Control, ControlKind>",
    )
    return all(row in rows for row in required)


def read_fixture(name: str) -> str:
    """Read one committed synthetic fixture."""
    return (FIXTURE_DIR / name).read_text(encoding="utf-8")


def sandbox_inventory(slot: Path, source: str) -> str:
    """Write `source` into the sandbox at `slot` and inventory it there.

    The baseline and the mutation of a case use the same `slot`, so the module
    prefix of every row is identical and only the surface change is compared.
    """
    slot.parent.mkdir(parents=True, exist_ok=True)
    slot.write_text(source, encoding="utf-8")
    return render(inventory_paths([slot]))


def run_self_test(keep: bool) -> int:
    """Prove the inventory detects each synthetic breakage and not an unchanged source."""
    sandbox = Path(tempfile.mkdtemp(prefix="api-snapshot-selftest-"))
    cases = MUTATIONS + (NEGATIVE_CONTROL,)
    passing = 0
    try:
        base = read_fixture(BASE_FIXTURE)
    except OSError as error:
        print(f"self-test harness error: cannot read fixtures: {error}", file=sys.stderr)
        shutil.rmtree(sandbox)
        return 2
    for position, (label, fixture) in enumerate(cases):
        slot = sandbox / f"case-{position:02d}" / "lib.rs"
        try:
            baseline = sandbox_inventory(slot, base)
            mutated = sandbox_inventory(slot, read_fixture(fixture))
        except OSError as error:
            print(f"self-test harness error: cannot read {fixture}: {error}", file=sys.stderr)
            shutil.rmtree(sandbox)
            return 2
        code, _ = guard_verdict(baseline, mutated)
        expect_drift = fixture != BASE_FIXTURE
        ok = (code != 0) if expect_drift else (code == 0)
        if ok:
            passing += 1
        observed = "drift reported" if code != 0 else "no drift"
        verdict = "PASS" if ok else "FAIL"
        expected = "expected drift" if expect_drift else "expected clean"
        print(f"{verdict}: {label} -> {observed} ({expected})")
    print(f"self-test: {passing}/{len(cases)} cases behaved as expected")
    if keep:
        print(f"sandbox kept at {sandbox}")
    else:
        shutil.rmtree(sandbox)
    return 0 if passing == len(cases) else 1


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Compare the public Rust surface with the checked-in snapshot.")
    parser.add_argument("--write", action="store_true", help="rewrite the snapshot from the current inventory")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run the in-process mutation harness; reads no repository artifact",
    )
    parser.add_argument(
        "--self-test-keep",
        action="store_true",
        help="with --self-test, keep the sandbox directory and print its path",
    )
    args = parser.parse_args(argv)

    if args.self_test or args.self_test_keep:
        return run_self_test(keep=args.self_test_keep)

    if not positive_control_passes():
        print("API snapshot positive control failed", file=sys.stderr)
        return 2

    current = render(inventory_paths(TRACKED_FILES))
    if args.write:
        SNAPSHOT_PATH.parent.mkdir(parents=True, exist_ok=True)
        SNAPSHOT_PATH.write_text(current, encoding="utf-8")
        print(f"updated {SNAPSHOT_PATH} ({len(current.splitlines())} rows)")
        return 0

    expected = SNAPSHOT_PATH.read_text(encoding="utf-8") if SNAPSHOT_PATH.exists() else ""
    code, messages = guard_verdict(expected, current)
    if code:
        print("\n".join(messages), file=sys.stderr)
        return code
    print(
        f"public API snapshot matches {SNAPSHOT_PATH} "
        f"({len(current.splitlines())} rows); positive control passed"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())