#!/usr/bin/env python3
"""Reject runtime/network dependencies in the runtime-neutral protocol crate."""
from pathlib import Path
import re
import sys

FORBIDDEN = re.compile(r"(?im)^\s*(tokio|async-std|smol|mio|socket2|hyper|reqwest)\s*=")


def check(path: Path) -> bool:
    text = path.read_text(encoding="utf-8")
    bad = FORBIDDEN.search(text)
    if bad:
        print(f"forbidden runtime/network dependency in {path}: {bad.group(1)}", file=sys.stderr)
        return False
    return True


def main() -> int:
    manifest = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("crates/i2pr-sam-proto/Cargo.toml")
    if not check(manifest):
        return 1
    positive_control = "[dependencies]\ntokio = { version = \"1\" }\n"
    if not FORBIDDEN.search(positive_control):
        print("boundary checker positive control failed", file=sys.stderr)
        return 2
    print(f"protocol boundary clean: {manifest}; positive control detected tokio")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
