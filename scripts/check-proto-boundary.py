#!/usr/bin/env python3
"""Reject runtime/network dependencies in the runtime-neutral protocol crate.

The SAM protocol crate must stay runtime-neutral so the blocking facade, the
async client, and any future binding can share one canonical codec. This guard
reads the crate's manifest and fails closed if a runtime or network dependency
appears in any dependency table.

Exit codes (unchanged for callers and CI):

  0  manifest is clean and the positive control still detects a violation
  1  a forbidden dependency was found
  2  the guard's own positive control failed, i.e. the guard is broken

Usage:

  python3 scripts/check-proto-boundary.py [manifest]   compare a manifest
  python3 scripts/check-proto-boundary.py --self-test  prove the detector fires
  python3 scripts/check-proto-boundary.py --self-test --self-test-keep

`--self-test` builds every forbidden dependency form, in inline-table and
plain-string spelling, in a throwaway sandbox under `tempfile.mkdtemp()`; it
never reads or writes a checked-in repository artifact other than the synthetic
fixtures it copies from `scripts/guards/mutations/boundary/`.

What this guard does NOT cover (deliberately not claimed):

* a forbidden crate declared only in the workspace root `[workspace.dependencies]`
  table is invisible here, because this guard reads one manifest; Cargo feature
  unification means that path still deserves a human or `cargo tree` check;
* `[target.'cfg(...)'.dependencies]` entries are matched by the same line
  anchored pattern, but a dependency pulled in through another crate's manifest
  is out of scope;
* build-time dependencies reached by a path or git dependency are not resolved;
* this is a manifest text guard, not a build-graph or semver tool.
"""
from __future__ import annotations

import argparse
from collections.abc import Sequence
from pathlib import Path
import re
import shutil
import sys
import tempfile

# The protocol crate must not depend on an async runtime, an event loop, a raw
# socket wrapper, or an HTTP client. Keep this list and the regexes below in
# sync: the self-test substitutes each name in this tuple into a sandbox copy of
# each manifest form, so a name added here is immediately covered.
FORBIDDEN_DEPENDENCIES: tuple[str, ...] = (
    "tokio",
    "async-std",
    "smol",
    "mio",
    "socket2",
    "hyper",
    "reqwest",
)

_NAME_GROUP = "|".join(re.escape(name) for name in FORBIDDEN_DEPENDENCIES)

# A dependency entry for a forbidden crate, anchored to the start of a line so
# only manifest keys match. The optional `.workspace` keeps `<crate>.workspace =
# true` (workspace inheritance) from evading the match.
FORBIDDEN_KEY = re.compile(
    rf"(?im)^[ \t]*({_NAME_GROUP})(?:[ \t]*\.[ \t]*workspace)?[ \t]*=",
)

# A forbidden crate renamed through a `package = "<name>"` key, which is the
# obvious way to hide one from a key-only match.
FORBIDDEN_PACKAGE = re.compile(
    rf'(?i)package[ \t]*=[ \t]*"({_NAME_GROUP})"',
)

# Positive control: proves the detector still matches a runtime dependency, so
# a clean run cannot be the result of a silently broken regex.
POSITIVE_CONTROL_TEXT = '[dependencies]\ntokio = { version = "1" }\n'

DEFAULT_MANIFEST = Path("crates/i2pr-sam-proto/Cargo.toml")
FIXTURE_DIR = Path(__file__).resolve().parent / "guards" / "mutations" / "boundary"

MANIFEST_FORMS: tuple[tuple[str, str], ...] = (
    ("inline-table", "forbidden_inline_table.toml"),
    ("plain-string", "forbidden_plain_string.toml"),
    ("workspace-inherited", "forbidden_workspace_inherited.toml"),
    ("renamed-package", "forbidden_renamed_package.toml"),
)


def forbidden_dependencies(text: str) -> list[tuple[str, str]]:
    """Return `(name, form)` for every forbidden dependency in manifest text.

    Pure function so the real run and the self-test share one detector.
    """
    found: set[tuple[str, str]] = set()
    for pattern, form in ((FORBIDDEN_KEY, "dependency key"), (FORBIDDEN_PACKAGE, "renamed package")):
        for match in pattern.finditer(text):
            found.add((match.group(1), form))
    return sorted(found)


def check_manifest_text(text: str, label: str, quiet: bool = False) -> bool:
    """Report every forbidden dependency in `text`; return True when clean.

    `quiet` only suppresses the human-facing diagnostics, which the self-test
    replaces with its own per-case line. The detection path is identical.
    """
    found = forbidden_dependencies(text)
    if not found:
        return True
    if not quiet:
        for name, form in found:
            print(
                f"forbidden runtime/network dependency in {label}: {name} ({form})",
                file=sys.stderr,
            )
    return False


def check_manifest(path: Path, quiet: bool = False) -> bool:
    """Read a manifest and run the shared detector over it."""
    return check_manifest_text(path.read_text(encoding="utf-8"), str(path), quiet)


def positive_control_passes() -> bool:
    """Confirm the detector still flags a synthetic runtime dependency."""
    return bool(forbidden_dependencies(POSITIVE_CONTROL_TEXT))


def _dependency_section(text: str) -> tuple[str, str]:
    """Split manifest text at `[dependencies]`, returning head and remainder."""
    head, separator, tail = text.partition("[dependencies]")
    if not separator:
        raise ValueError("fixture has no [dependencies] section")
    return head + separator, tail


def substitute_crate_token(text: str, name: str) -> str:
    """Rewrite the crate token of the first entry after `[dependencies]`.

    Applied to a sandbox copy of a fixture so one committed manifest shape
    covers every forbidden name, including `<crate>.workspace = true`.
    """
    head, tail = _dependency_section(text)
    rewritten, count = re.subn(r"(?m)^[A-Za-z0-9_.-]+", name, tail, count=1)
    if count != 1:
        raise ValueError("fixture dependency entry not found")
    return head + rewritten


def substitute_package_value(text: str, name: str) -> str:
    """Rewrite the quoted `package = "<name>"` value in a fixture manifest."""
    rewritten, count = re.subn(
        r'(?m)(package[ \t]*=[ \t]*")[A-Za-z0-9_-]+(")',
        rf"\g<1>{name}\g<2>",
        text,
        count=1,
    )
    if count != 1:
        raise ValueError("fixture package rename not found")
    return rewritten


def self_test_cases() -> list[tuple[str, str, str, str, str]]:
    """Return `(label, crate_name, fixture, form, expected)` mutation cases.

    `expected` is `"clean"` for the negative control and `"forbidden"` for every
    case the detector must flag.
    """
    cases: list[tuple[str, str, str, str, str]] = [
        ("negative control", "", "clean.toml", "clean", "clean"),
    ]
    for name in FORBIDDEN_DEPENDENCIES:
        for form, fixture in MANIFEST_FORMS:
            cases.append((f"{name} as {form}", name, fixture, form, "forbidden"))
    return cases


def run_self_test(keep: bool) -> int:
    """Prove the detector flags every forbidden manifest form and accepts a clean one.

    Each case is materialized in a sandbox directory copied from the committed
    fixtures, so no repository artifact other than those read-only fixtures is
    touched.
    """
    sandbox = Path(tempfile.mkdtemp(prefix="proto-boundary-selftest-"))
    cases = self_test_cases()
    passing = 0
    for label, name, fixture, form, expected in cases:
        fixture_text = (FIXTURE_DIR / fixture).read_text(encoding="utf-8")
        manifest = sandbox / fixture
        if form == "renamed-package":
            manifest.write_text(substitute_package_value(fixture_text, name), encoding="utf-8")
        elif form == "clean":
            manifest.write_text(fixture_text, encoding="utf-8")
        else:
            manifest.write_text(substitute_crate_token(fixture_text, name), encoding="utf-8")
        observed = "clean" if check_manifest(manifest, quiet=True) else "forbidden"
        verdict = "PASS" if observed == expected else "FAIL"
        if verdict == "PASS":
            passing += 1
        print(f"{verdict}: {label} -> {observed}")
    print(f"self-test: {passing}/{len(cases)} cases behaved as expected")
    if keep:
        print(f"sandbox kept at {sandbox}")
    else:
        shutil.rmtree(sandbox)
    return 0 if passing == len(cases) else 1


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Reject runtime dependencies in the protocol crate.")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run the in-process mutation harness instead of checking a manifest",
    )
    parser.add_argument(
        "--self-test-keep",
        action="store_true",
        help="with --self-test, keep the sandbox directory and print its path",
    )
    parser.add_argument(
        "manifest",
        nargs="?",
        default=None,
        help=f"manifest to check (default: {DEFAULT_MANIFEST})",
    )
    args = parser.parse_args(argv)

    if args.self_test or args.self_test_keep:
        return run_self_test(keep=args.self_test_keep)

    manifest = Path(args.manifest) if args.manifest else DEFAULT_MANIFEST
    if not positive_control_passes():
        print("boundary checker positive control failed", file=sys.stderr)
        return 2
    if not check_manifest(manifest):
        return 1
    print(f"protocol boundary clean: {manifest}; positive control detected tokio")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())