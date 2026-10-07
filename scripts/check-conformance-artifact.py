#!/usr/bin/env python3
"""Validate a SAM conformance artifact against the repository schema.

The point of this script is to make an overclaiming artifact fail. A snapshot check that
can only observe token text cannot tell a real payload exchange from a session that merely
existed, so the artifact rules are enforced here and cross-checked independently of the
runner that produced them:

* `pass` requires payload evidence: bytes sent, bytes received, and an exact byte match;
* `create_only` may never carry payload bytes and must say why it has no evidence;
* `not_run` must name a diagnostic category;
* a shared-session pass must carry a concrete Destination hash and proven identity;
* the summary must agree with the rows, and `create_only` never counts as a capability pass;
* private key material must not appear anywhere in the artifact.

Exit codes: 0 valid, 1 artifact invalid, 2 usage or unreadable schema.

Only the JSON Schema subset used by specs/conformance.schema.json is implemented, and that
subset is documented in SUPPORTED_KEYWORDS below. The validator is deliberately strict:
an unknown keyword is an error rather than a silent pass.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SCHEMA = REPO_ROOT / "specs" / "conformance.schema.json"

SUPPORTED_KEYWORDS = {
    "$schema",
    "$id",
    "$ref",
    "$defs",
    "title",
    "description",
    "type",
    "const",
    "enum",
    "required",
    "properties",
    "additionalProperties",
    "items",
    "minItems",
    "minLength",
    "minimum",
    "maximum",
    "pattern",
    "not",
    "allOf",
    "anyOf",
    "oneOf",
    "if",
    "then",
    "else",
}

RESULTS = ("pass", "unsupported", "fail", "not_run", "create_only")
PRIVATE_MATERIAL = re.compile(r"PRIV=|privkey|private[_-]?key", re.IGNORECASE)


class Invalid(Exception):
    """Raised with every problem found, so one run reports all of them."""


def check_type(value: Any, expected: str, where: str, problems: list[str]) -> None:
    """Validate one JSON Schema primitive type."""
    actual = {
        "object": dict,
        "array": list,
        "string": str,
        "integer": int,
        "number": (int, float),
        "boolean": bool,
        "null": type(None),
    }.get(expected)
    if actual is None:
        problems.append(f"{where}: unsupported schema type {expected!r}")
        return
    # JSON has no separate bool-as-int rule here: bools are never integers.
    if expected in ("integer", "number") and isinstance(value, bool):
        problems.append(f"{where}: expected {expected}, found boolean")
        return
    if not isinstance(value, actual):
        problems.append(
            f"{where}: expected {expected}, found {type(value).__name__}"
        )


def validate(value: Any, schema: dict[str, Any], root: dict[str, Any], where: str, problems: list[str]) -> None:
    """Validate one schema node; unsupported keywords are reported, never ignored."""
    unknown = set(schema) - SUPPORTED_KEYWORDS
    if unknown:
        problems.append(f"{where}: schema uses unimplemented keywords {sorted(unknown)}")
        return

    if "$ref" in schema:
        target = resolve_ref(schema["$ref"], root, where, problems)
        if target is not None:
            validate(value, target, root, where, problems)
        return

    if "type" in schema:
        expected = schema["type"]
        options = expected if isinstance(expected, list) else [expected]
        if not any(is_type(value, option) for option in options):
            problems.append(f"{where}: expected type {expected}, found {type(value).__name__}")
            return

    if "const" in schema and value != schema["const"]:
        problems.append(f"{where}: expected constant {schema['const']!r}, found {value!r}")
    if "enum" in schema and value not in schema["enum"]:
        problems.append(f"{where}: {value!r} is not one of {schema['enum']}")

    if isinstance(value, str):
        if "minLength" in schema and len(value) < schema["minLength"]:
            problems.append(f"{where}: shorter than minLength {schema['minLength']}")
        if "pattern" in schema and not re.search(schema["pattern"], value):
            problems.append(f"{where}: {value!r} does not match {schema['pattern']!r}")
        if "not" in schema and "pattern" in schema["not"]:
            if re.search(schema["not"]["pattern"], value):
                problems.append(f"{where}: matches forbidden pattern {schema['not']['pattern']!r}")

    if isinstance(value, (int, float)) and not isinstance(value, bool):
        if "minimum" in schema and value < schema["minimum"]:
            problems.append(f"{where}: {value} is below minimum {schema['minimum']}")
        if "maximum" in schema and value > schema["maximum"]:
            problems.append(f"{where}: {value} is above maximum {schema['maximum']}")

    if isinstance(value, dict):
        for key in schema.get("required", []):
            if key not in value:
                problems.append(f"{where}: missing required property {key!r}")
        properties = schema.get("properties", {})
        for key, subschema in properties.items():
            if key in value:
                validate(value[key], subschema, root, f"{where}.{key}", problems)
        if schema.get("additionalProperties") is False:
            for key in value:
                if key not in properties:
                    problems.append(f"{where}: unexpected property {key!r}")

    if isinstance(value, list):
        if "minItems" in schema and len(value) < schema["minItems"]:
            problems.append(f"{where}: fewer than minItems {schema['minItems']}")
        if "items" in schema:
            for index, item in enumerate(value):
                validate(item, schema["items"], root, f"{where}[{index}]", problems)

    for subschema in schema.get("allOf", []):
        if not matches(value, subschema, root):
            problems.append(f"{where}: failed an allOf branch")
    if "anyOf" in schema and not any(matches(value, sub, root) for sub in schema["anyOf"]):
        problems.append(f"{where}: matched no anyOf branch")

    if "oneOf" in schema:
        matched = sum(1 for sub in schema["oneOf"] if matches(value, sub, root))
        if matched != 1:
            problems.append(f"{where}: matched {matched} oneOf branches, expected exactly 1")

    if "if" in schema:
        branch = "then" if matches(value, schema["if"], root) else "else"
        if branch in schema:
            validate(value, schema[branch], root, where, problems)

    if "not" in schema and matches(value, schema["not"], root):
        problems.append(f"{where}: matched a schema it must not match")


def matches(value: Any, schema: Any, root: dict[str, Any]) -> bool:
    """Report whether one schema accepts one value, collecting detail on the side."""
    problems: list[str] = []
    validate(value, schema, root, "$probe", problems)
    return not problems


def is_type(value: Any, expected: str) -> bool:
    if expected == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if expected == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if expected == "boolean":
        return isinstance(value, bool)
    if expected == "null":
        return value is None
    if expected == "string":
        return isinstance(value, str)
    if expected == "array":
        return isinstance(value, list)
    if expected == "object":
        return isinstance(value, dict)
    return False


def resolve_ref(ref: str, root: dict[str, Any], where: str, problems: list[str]) -> dict[str, Any] | None:
    """Resolve a local `#/$defs/...` pointer; remote refs are refused rather than guessed."""
    if not ref.startswith("#/"):
        problems.append(f"{where}: only local $ref pointers are supported, got {ref!r}")
        return None
    node: Any = root
    for token in ref[2:].split("/"):
        token = token.replace("~1", "/").replace("~0", "~")
        if not isinstance(node, dict) or token not in node:
            problems.append(f"{where}: cannot resolve $ref {ref!r}")
            return None
        node = node[token]
    if not isinstance(node, dict):
        problems.append(f"{where}: $ref {ref!r} does not point at a schema")
        return None
    return node


def cross_check(artifact: dict[str, Any]) -> list[str]:
    """Check invariants the schema cannot express: summary arithmetic and key hygiene."""
    problems: list[str] = []
    rows = artifact.get("rows", [])
    summary = artifact.get("summary", {})
    if not isinstance(rows, list) or not isinstance(summary, dict):
        return problems

    counts = {result: 0 for result in RESULTS}
    for row in rows:
        if not isinstance(row, dict):
            continue
        result = row.get("result")
        if isinstance(result, str) and result in counts:
            counts[result] += 1
        evidence = row.get("evidence", {})
        if isinstance(evidence, dict):
            if result != "pass" and (
                evidence.get("bytes_sent", 0) or evidence.get("bytes_received", 0)
            ):
                problems.append(
                    f"row {row.get('operation')!r} reports {result} while carrying payload bytes; "
                    "only a verified exchange may move payload"
                )
            if result == "pass" and (
                evidence.get("bytes_sent", 0) < 1 or evidence.get("bytes_received", 0) < 1
            ):
                problems.append(
                    f"row {row.get('operation')!r} claims pass without payload in both directions"
                )
        if result == "pass" and row.get("feature") == "shared":
            identity = row.get("local_identity")
            if not isinstance(identity, dict) or not identity.get("destination_hash"):
                problems.append(
                    f"shared row {row.get('operation')!r} claims pass without a concrete Destination"
                )

    for result, expected in counts.items():
        recorded = summary.get(result)
        if recorded != expected:
            problems.append(
                f"summary.{result} is {recorded!r} but the rows contain {expected}"
            )
    passes = counts["pass"]
    if summary.get("capability_passes") != passes:
        problems.append(
            f"summary.capability_passes is {summary.get('capability_passes')!r} but "
            f"{passes} row(s) carry payload evidence; create_only must never be counted"
        )

    if PRIVATE_MATERIAL.search(json.dumps(artifact)):
        problems.append("artifact contains private key material; publish hashes and public Destinations only")
    return problems


def load(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read {path}: {error}")


def validate_artifact(artifact: Any, schema: dict[str, Any]) -> list[str]:
    problems: list[str] = []
    validate(artifact, schema, schema, "$", problems)
    if isinstance(artifact, dict):
        problems.extend(cross_check(artifact))
    return problems


def self_test(schema: dict[str, Any]) -> int:
    """Prove the validator rejects artifacts that overclaim, not just malformed ones."""
    passing = {
        "schema_version": "1.1",
        "router": "Java I2P",
        "router_version_or_sha": "a629ec7c9c675dd252d005fb881efd92e8e6ba27",
        "endpoint": "127.0.0.1:7656",
        "negotiated_sam_version": "3.3",
        "generated_at": "2026-10-07T15:00:00Z",
        "plan": "full",
        "capabilities": {name: "unknown" for name in (
            "stream", "datagram", "raw", "datagram2", "datagram3",
            "shared_primary", "shared_master", "datagram_direct", "raw_direct",
            "session_identity_lookup",
        )},
        "rows": [
            {
                "feature": "stream",
                "operation": "stream_payload_bidirectional",
                "shared_dialect": None,
                "local_identity": {
                    "destination": "ZmFrZS1kZXN0aW5hdGlvbi12YWx1ZQ==",
                    "destination_hash": "a" * 64,
                },
                "peer_identity": {
                    "destination": "ZmFrZS1wZWVyLWRlc3RpbmF0aW9u",
                    "destination_hash": "b" * 64,
                },
                "result": "pass",
                "evidence": {
                    "bytes_sent": 512,
                    "bytes_received": 512,
                    "exact_payload_match": True,
                    "identity_proven": True,
                },
                "diagnostic_category": None,
                "notes": "exact bidirectional payload",
            }
        ],
        "summary": {
            "pass": 1,
            "unsupported": 0,
            "fail": 0,
            "not_run": 0,
            "create_only": 0,
            "capability_passes": 1,
        },
    }
    import copy

    failures: list[str] = []

    def expect(label: str, artifact: Any, should_pass: bool) -> None:
        problems = validate_artifact(artifact, schema)
        accepted = not problems
        if accepted != should_pass:
            failures.append(
                f"{label}: expected {'accept' if should_pass else 'reject'}, "
                f"got {'accept' if accepted else 'reject'} ({problems[:2]})"
            )
        print(f"{'PASS' if accepted == should_pass else 'FAIL'}: {label}")

    expect("well-formed payload pass is accepted", copy.deepcopy(passing), True)

    no_bytes = copy.deepcopy(passing)
    no_bytes["rows"][0]["evidence"]["bytes_received"] = 0
    no_bytes["rows"][0]["evidence"]["bytes_sent"] = 0
    expect("pass without payload is rejected", no_bytes, False)

    mismatch = copy.deepcopy(passing)
    mismatch["rows"][0]["evidence"]["exact_payload_match"] = False
    expect("pass with a payload mismatch is rejected", mismatch, False)

    created = copy.deepcopy(passing)
    created["rows"][0]["result"] = "create_only"
    created["rows"][0]["evidence"] = {
        "bytes_sent": 0,
        "bytes_received": 0,
        "exact_payload_match": False,
        "identity_proven": False,
    }
    created["rows"][0]["diagnostic_category"] = "no_payload_evidence"
    created["summary"] = {
        "pass": 0, "unsupported": 0, "fail": 0, "not_run": 0, "create_only": 1,
        "capability_passes": 0,
    }
    expect("create_only is accepted without payload evidence", created, True)

    counted = copy.deepcopy(created)
    counted["summary"]["capability_passes"] = 1
    expect("create_only counted as a capability pass is rejected", counted, False)

    silent = copy.deepcopy(passing)
    silent["rows"][0]["result"] = "not_run"
    silent["rows"][0]["diagnostic_category"] = None
    silent["rows"][0]["evidence"] = {
        "bytes_sent": 0, "bytes_received": 0,
        "exact_payload_match": False, "identity_proven": False,
    }
    silent["summary"] = {
        "pass": 0, "unsupported": 0, "fail": 0, "not_run": 1, "create_only": 0,
        "capability_passes": 0,
    }
    expect("not_run without a diagnostic category is rejected", silent, False)

    shared_token = copy.deepcopy(passing)
    shared_token["rows"][0]["feature"] = "shared"
    shared_token["rows"][0]["shared_dialect"] = None
    expect("shared pass without a dialect is rejected", shared_token, False)

    shared_no_identity = copy.deepcopy(passing)
    shared_no_identity["rows"][0]["feature"] = "shared"
    shared_no_identity["rows"][0]["shared_dialect"] = "PRIMARY"
    shared_no_identity["rows"][0]["local_identity"] = None
    expect("shared pass without a concrete Destination is rejected", shared_no_identity, False)

    unproven = copy.deepcopy(passing)
    unproven["rows"][0]["feature"] = "shared"
    unproven["rows"][0]["shared_dialect"] = "PRIMARY"
    unproven["rows"][0]["evidence"]["identity_proven"] = False
    expect("shared pass without proven identity is rejected", unproven, False)

    leaky = copy.deepcopy(passing)
    leaky["rows"][0]["notes"] = "captured PRIV=aaaa from SESSION STATUS"
    expect("private key material in notes is rejected", leaky, False)

    miscounted = copy.deepcopy(passing)
    miscounted["summary"]["pass"] = 5
    expect("summary that disagrees with rows is rejected", miscounted, False)

    unknown_result = copy.deepcopy(passing)
    unknown_result["rows"][0]["result"] = "probably-fine"
    expect("unknown result vocabulary is rejected", unknown_result, False)

    if failures:
        print(f"self-test: {len(failures)} case(s) misbehaved")
        for failure in failures:
            print(f"  {failure}")
        return 1
    print("self-test: every case behaved as expected")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("artifact", nargs="?", type=Path, help="artifact JSON file to validate")
    parser.add_argument("--schema", type=Path, default=DEFAULT_SCHEMA, help="schema to validate against")
    parser.add_argument("--self-test", action="store_true", help="prove the validator rejects overclaiming artifacts")
    args = parser.parse_args(argv)

    schema = load(args.schema)
    if args.self_test:
        return self_test(schema)
    if args.artifact is None:
        print("usage: check-conformance-artifact.py ARTIFACT | --self-test", file=sys.stderr)
        return 2
    artifact = load(args.artifact)
    problems = validate_artifact(artifact, schema)
    if problems:
        print(f"invalid conformance artifact {args.artifact}: {len(problems)} problem(s)")
        for problem in problems:
            print(f"  {problem}")
        return 1
    print(f"conformance artifact valid: {args.artifact}")
    return 0


if __name__ == "__main__":
    sys.exit(main())