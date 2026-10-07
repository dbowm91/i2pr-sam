#!/usr/bin/env python3
"""Review public Rust declarations and fields through a checked-in source snapshot."""
from pathlib import Path
import re
import sys

FILES = [
    Path("crates/i2pr-sam/src/lib.rs"),
    Path("crates/i2pr-sam-proto/src/lib.rs"),
    Path("crates/i2pr-sam-blocking/src/lib.rs"),
]
ITEM = re.compile(r"^\s*pub\s+(?:(?:async|unsafe|const)\s+)*(fn|struct|enum|trait|type|const|static|use)\b")
FN = re.compile(r"^\s*pub\s+(?:(?:async|unsafe|const)\s+)*fn\s+([A-Za-z_][A-Za-z0-9_]*)")
TYPE = re.compile(r"^\s*pub\s+(?:struct|enum|trait|type)\s+([A-Za-z_][A-Za-z0-9_]*)")


def declaration(lines: list[str], start: int) -> tuple[str, int]:
    """Collect a declaration header through its opening body or terminating semicolon."""
    parts = []
    parens = brackets = 0
    for index in range(start, len(lines)):
        line = lines[index].strip()
        parts.append(line)
        parens += line.count("(") - line.count(")")
        brackets += line.count("[") - line.count("]")
        if parens <= 0 and brackets <= 0 and ("{" in line or ";" in line):
            break
    return " ".join(" ".join(parts).split()), index


def inventory_text(text: str, module: str) -> set[str]:
    lines = text.splitlines()
    found: set[str] = set()
    i = 0
    while i < len(lines):
        line = lines[i]
        item = ITEM.match(line)
        fn = FN.match(line)
        ty = TYPE.match(line)
        if item and fn:
            header, end = declaration(lines, i)
            found.add(f"{module}::{header}")
            i = end + 1
            continue
        if item and ty:
            header, end = declaration(lines, i)
            found.add(f"{module}::{header}")
            if "{" in lines[end]:
                depth = 0
                kind = item.group(1)
                for body_line in lines[end:]:
                    depth += body_line.count("{") - body_line.count("}")
                    stripped = body_line.strip()
                    if kind == "struct" and stripped.startswith("pub "):
                        found.add(f"{module}::{ty.group(1)} field {stripped.rstrip(',')}")
                    elif kind == "enum" and depth >= 1 and stripped not in ("{", "}") and not stripped.startswith("//"):
                        found.add(f"{module}::{ty.group(1)} variant {stripped.rstrip(',')}")
                    if depth == 0:
                        break
            i = end + 1
            continue
        if item:
            header, end = declaration(lines, i)
            found.add(f"{module}::{header}")
            i = end + 1
            continue
        i += 1
    return found


def inventory() -> str:
    entries = set()
    for path in FILES:
        entries.update(inventory_text(path.read_text(encoding="utf-8"), path.as_posix()))
    return "\n".join(sorted(entries)) + "\n"


def main() -> int:
    expected = Path("api/public-types.txt")
    current = inventory()
    if len(sys.argv) > 1 and sys.argv[1] == "--write":
        expected.parent.mkdir(parents=True, exist_ok=True)
        expected.write_text(current, encoding="utf-8")
        print(f"updated {expected}")
        return 0
    if not expected.exists() or expected.read_text(encoding="utf-8") != current:
        print("public API declarations differ; review and run scripts/check-api-snapshot.py --write", file=sys.stderr)
        return 1
    control = inventory_text("pub struct PositiveControl;\n", "control")
    if not any("PositiveControl" in entry for entry in control):
        print("API snapshot positive control failed", file=sys.stderr)
        return 2
    print(f"public API declaration snapshot matches {expected}; positive control passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
