#!/usr/bin/env python3
"""Router-enabled interoperability qualification driver.

This script owns no protocol logic. It resolves a router pin from
``routers.json``, proves that the live prerequisites for that router exist,
invokes the repository conformance runner, validates whatever JSON artifact the
runner produced, and merges the results into one comparable matrix.

Two rules shape the whole design:

1. The harness never upgrades a result. A row is ``pass`` only when the runner
   emitted ``pass``; when the live lane cannot start, every row is synthesized
   as ``not_run`` with the blocking diagnostic category attached. A missing
   router is not a compatibility failure and must never read like one.
2. Every failure path names a diagnostic category. Silence would let an
   unprovisioned environment look like a qualified one, which is exactly the
   failure mode milestone 005 forbids.

Exit codes: 0 every requested router produced a valid artifact with no ``fail``
rows; 1 a ``fail`` row or an invalid artifact; 3 a requested router could not
start its live lane for a named provisioning reason; 2 usage error.
"""
from __future__ import annotations

import argparse
import csv
import json
import os
import shutil
import socket
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import datetime, timezone
from functools import partial
from pathlib import Path
from typing import Any, Callable, Iterable

HARNESS_DIR = Path(__file__).resolve().parent
REPO_ROOT = HARNESS_DIR.parent.parent
ROUTERS_CONFIG = HARNESS_DIR / "routers.json"
UDP_PROBE = HARNESS_DIR / "udp_egress_probe.py"
ARTIFACT_CHECKER = REPO_ROOT / "scripts" / "check-conformance-artifact.py"
RUNNER_PACKAGE = "i2pr-sam"
RUNNER_BIN = "sam-conformance"
SCHEMA_VERSION = "1.1"
UDP_PROBE_TIMEOUT = 5.0

VALID_RESULTS = ("pass", "unsupported", "fail", "not_run", "create_only")

# Feature set per plan. A lane that cannot start must still account for every
# feature the plan asked about, otherwise a blocked run looks like a partial run.
# A lane that cannot start must still account for every feature the plan asked about.
# Feature and shared dialect are separate fields in the artifact schema: the dialect is a
# property of a shared session, not a separate kind of capability.
PLAN_FEATURES: dict[str, tuple[tuple[str, str | None], ...]] = {
    "full": (
        ("stream", None),
        ("datagram", None),
        ("raw", None),
        ("datagram2", None),
        ("datagram3", None),
        ("shared", "PRIMARY"),
        ("shared", "MASTER"),
    ),
    "stream": (("stream", None),),
    "datagram": (("datagram", None), ("raw", None), ("datagram2", None), ("datagram3", None)),
    "shared": (("shared", "PRIMARY"), ("shared", "MASTER")),
}

MATRIX_HEADER = (
    "router",
    "router_version_or_sha",
    "negotiated_sam_version",
    "feature",
    "operation",
    "shared_dialect",
    "result",
    "diagnostic_category",
    "local_destination_hash",
    "notes",
)

# Diagnostic categories. Every non-pass condition maps onto exactly one of these.
DIAG_BINARY_MISSING = "provisioning_binary_missing"
DIAG_PORT_CLOSED = "provisioning_port_closed"
DIAG_UDP_BLOCKED = "provisioning_udp_egress_blocked"
DIAG_BRIDGE_UNREACHABLE = "provisioning_bridge_unreachable"
DIAG_RUNNER_CLI = "runner_cli_mismatch"
DIAG_RUNNER_CONNECTION = "runner_connection_error"
DIAG_RUNNER_PROVISIONING = "runner_provisioning_error"
DIAG_RUNNER_TIMEOUT = "runner_timeout"
DIAG_RUNNER_MISSING = "runner_binary_missing"
DIAG_RUNNER_BUILD = "runner_build_failed"
DIAG_RUNNER_ERROR = "runner_error"
DIAG_ARTIFACT_INVALID = "artifact_schema_invalid"
DIAG_ARTIFACT_ABSENT = "artifact_absent"


def now_rfc3339() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def slug(label: str) -> str:
    return "".join(c if c.isalnum() else "-" for c in label.lower()).strip("-")


# --------------------------------------------------------------------------
# configuration
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class Router:
    label: str
    repo_url: str
    pinned_revision: str
    bridge_endpoint: str
    udp_endpoint: str
    binary: str | None
    binary_probe: str
    provisioning: dict[str, Any]
    env: dict[str, str]
    notes: str


@dataclass
class Outcome:
    """Everything the summary and the matrix need to know about one router."""

    router: Router
    endpoint: str
    plan: str
    artifact: dict[str, Any]
    artifact_path: Path | None
    artifact_valid: bool
    validator: str
    diagnostics: list[dict[str, str]] = field(default_factory=list)
    runner_exit: int | None = None
    runner_stderr: str = ""

    @property
    def rows(self) -> list[dict[str, Any]]:
        rows = self.artifact.get("rows")
        return rows if isinstance(rows, list) else []

    def has_fail_rows(self) -> bool:
        return any(row.get("result") == "fail" for row in self.rows)

    def blocking_diagnostic(self) -> str | None:
        """First named diagnostic that prevented the live lane, if any."""
        for entry in self.diagnostics:
            if entry.get("blocking") == "true":
                return entry.get("category") or DIAG_RUNNER_PROVISIONING
        return None


def load_routers(path: Path) -> dict[str, Router]:
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"interop: cannot read router config {path}: {error}")
    routers: dict[str, Router] = {}
    for entry in raw.get("routers", []):
        provisioning = dict(entry.get("provisioning") or {})
        router = Router(
            label=str(entry["label"]),
            repo_url=str(entry["repo_url"]),
            pinned_revision=str(entry["pinned_revision"]),
            bridge_endpoint=str(entry.get("bridge_endpoint", "127.0.0.1:7656")),
            udp_endpoint=str(entry.get("udp_endpoint", "127.0.0.1:7655")),
            binary=entry.get("binary"),
            binary_probe=str(entry.get("binary_probe", "path")),
            provisioning=provisioning,
            env=dict(entry.get("env") or {}),
            notes=str(entry.get("notes", "")),
        )
        routers[router.label] = router
    if not routers:
        raise SystemExit(f"interop: no routers defined in {path}")
    return routers


def resolve_routers(routers: dict[str, Router], labels: list[str], use_all: bool) -> list[Router]:
    if use_all:
        return list(routers.values())
    if not labels:
        raise SystemExit("interop: pass --router LABEL (repeatable) or --all")
    selected: list[Router] = []
    for label in labels:
        match = next((r for r in routers.values() if r.label.lower() == label.lower()), None)
        if match is None:
            raise SystemExit(f"interop: unknown router {label!r}; known: {', '.join(routers)}")
        selected.append(match)
    return selected


# --------------------------------------------------------------------------
# prerequisite probes
# --------------------------------------------------------------------------


def _diag(category: str, detail: str, blocking: bool = False) -> dict[str, str]:
    return {"category": category, "detail": detail, "blocking": "true" if blocking else "false"}


def probe_binary(router: Router) -> dict[str, str] | None:
    """Java I2P ships no single binary, so its LANE is checked at the bridge only."""
    if router.binary_probe == "bridge_only" or router.binary is None:
        return _diag(DIAG_BINARY_MISSING, "router has no single-binary entry point; bridge reachability is the gate")
    resolved = shutil.which(router.binary)
    if resolved is None:
        return _diag(
            DIAG_BINARY_MISSING,
            f"{router.binary!r} not found on PATH; documented start: {'; '.join(router.provisioning.get('start_commands', [])) or 'none recorded'}",
            blocking=True,
        )
    return _diag(DIAG_BINARY_MISSING, f"found {resolved}")


def probe_tcp_port(endpoint: str, timeout: float) -> dict[str, str]:
    host, _, port = endpoint.rpartition(":")
    try:
        with socket.create_connection((host, int(port)), timeout=timeout):
            return _diag(DIAG_PORT_CLOSED, f"TCP listener present on {endpoint}")
    except (OSError, ValueError) as error:
        return _diag(DIAG_PORT_CLOSED, f"no TCP listener on {endpoint}: {type(error).__name__}: {error}", blocking=True)


def udp_reachability(result: dict[str, Any]) -> str:
    """Normalize evidence to reachable/unreachable/unknown without inferring from silence."""
    if result.get("reachability") == "reachable" or result.get("reachable"):
        return "reachable"
    if result.get("reachability") == "unreachable" and result.get("unreachable_evidence"):
        return "unreachable"
    return "unknown"


def probe_udp_egress(timeout: float = UDP_PROBE_TIMEOUT) -> dict[str, str]:
    """Return advisory UDP evidence; arbitrary remote silence is inconclusive."""
    try:
        completed = subprocess.run(
            [sys.executable, str(UDP_PROBE), "--timeout", str(timeout)],
            capture_output=True,
            text=True,
            timeout=max(timeout * 2 + 5.0, 10.0),
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        return _diag(DIAG_UDP_BLOCKED, f"UDP probe unavailable (advisory): {type(error).__name__}: {error}")
    try:
        result = json.loads(completed.stdout.strip() or "{}")
    except json.JSONDecodeError as error:
        return _diag(DIAG_UDP_BLOCKED, f"UDP probe returned non-JSON output (advisory): {error}")
    reachable = result.get("reachable") or []
    reachability = udp_reachability(result)
    if reachability == "reachable":
        detail = f"UDP request/reply observed at {reachable}; this does not prove general peer reachability"
    elif reachability == "unreachable":
        detail = f"UDP unreachable with positive network evidence: {result['unreachable_evidence']} (advisory)"
    else:
        detail = "UDP reachability unknown: arbitrary endpoint silence cannot distinguish filtering from remote silence"
    return _diag(DIAG_UDP_BLOCKED, detail)


def probe_bridge(endpoint: str, timeout: float) -> dict[str, str]:
    """A listener that refuses the SAM handshake is still not a usable bridge."""
    host, _, port = endpoint.rpartition(":")
    try:
        with socket.create_connection((host, int(port)), timeout=timeout) as sock:
            sock.settimeout(timeout)
            sock.sendall(b"HELLO VERSION MIN=3.0 MAX=3.3\n")
            data = sock.recv(512)
    except (OSError, ValueError) as error:
        return _diag(DIAG_BRIDGE_UNREACHABLE, f"SAM bridge handshake failed on {endpoint}: {type(error).__name__}: {error}", blocking=True)
    if not data.startswith(b"HELLO REPLY"):
        return _diag(DIAG_BRIDGE_UNREACHABLE, f"listener on {endpoint} did not answer with HELLO REPLY: {data[:64]!r}", blocking=True)
    return _diag(DIAG_BRIDGE_UNREACHABLE, f"SAM bridge answered on {endpoint}")


# --------------------------------------------------------------------------
# conformance runner
# --------------------------------------------------------------------------


def runner_command(
    router: Router,
    endpoint: str,
    plan: str,
    output: Path,
    peer_endpoint: str | None = None,
    service_destination: str | None = None,
    control_timeout: int = 120,
    datagram_endpoint: str | None = None,
) -> list[str]:
    argv = [
        "cargo",
        "run",
        "--locked",
        "--release",
        "-p",
        RUNNER_PACKAGE,
        "--bin",
        RUNNER_BIN,
        "--",
        "--endpoint",
        endpoint,
        "--router",
        router.label,
        "--router-version",
        router.pinned_revision,
        "--plan",
        plan,
        "--output",
        str(output),
        "--control-timeout",
        str(control_timeout),
    ]
    if peer_endpoint:
        # The peer may use this same router through another SAM connection; it need not
        # belong to a second router implementation.
        argv.extend(["--peer-endpoint", peer_endpoint])
    if service_destination:
        argv.extend(["--service-destination", service_destination])
    if datagram_endpoint:
        argv.extend(["--datagram-endpoint", datagram_endpoint])
    return argv


def run_runner(
    router: Router,
    endpoint: str,
    plan: str,
    output: Path,
    timeout: float,
    peer_endpoint: str | None = None,
    service_destination: str | None = None,
    control_timeout: int = 120,
    datagram_endpoint: str | None = None,
) -> tuple[int | None, str, str, dict[str, str] | None]:

    """Return (exit_code, stdout, stderr, diagnostic); diagnostic is set when the run failed."""
    argv = runner_command(
        router, endpoint, plan, output, peer_endpoint, service_destination, control_timeout,
        datagram_endpoint,
    )
    if shutil.which("cargo") is None:
        return None, "", "", _diag(DIAG_RUNNER_MISSING, "cargo not found on PATH; cannot build sam-conformance", blocking=True)
    try:
        completed = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as expired:
        stderr = (expired.stderr or "") if isinstance(expired.stderr, str) else ""
        return None, "", stderr, _diag(DIAG_RUNNER_TIMEOUT, f"sam-conformance exceeded {timeout:g}s", blocking=True)
    except OSError as error:
        return None, "", "", _diag(DIAG_RUNNER_MISSING, f"cannot execute sam-conformance: {type(error).__name__}: {error}", blocking=True)

    stderr = completed.stderr or ""
    # Cargo's compile chatter is not diagnostic material; the runner's own last
    # line is what names the failure.
    runner_tail = [line for line in stderr.strip().splitlines() if line.strip()][-1:] or [""]
    detail = runner_tail[0].strip()[:300] or completed.stdout.strip()[:300]
    lowered = (stderr + completed.stdout).lower()
    if completed.returncode == 2 and ("unknown argument" in lowered or "unexpected argument" in lowered or "unrecognized" in lowered):
        # The runner CLI is still being upgraded; a flag it does not know is a
        # harness/runner skew, never evidence of a protocol result.
        return completed.returncode, completed.stdout, stderr, _diag(
            DIAG_RUNNER_CLI, f"sam-conformance rejected harness CLI flags (exit 2): {detail}", blocking=True
        )
    if completed.returncode == 2:
        return completed.returncode, completed.stdout, stderr, _diag(
            DIAG_RUNNER_CONNECTION, f"sam-conformance usage/connection error: {detail}", blocking=True
        )
    if completed.returncode == 3:
        return completed.returncode, completed.stdout, stderr, _diag(
            DIAG_RUNNER_PROVISIONING, f"sam-conformance reported a provisioning error: {detail}", blocking=True
        )
    if "could not compile" in lowered:
        # A broken build is a repository condition, not a router result.
        return completed.returncode, completed.stdout, stderr, _diag(
            DIAG_RUNNER_BUILD, f"cargo could not build sam-conformance: {detail}", blocking=True
        )
    if completed.returncode not in (0, 1):
        # 0 and 1 are documented runner outcomes; anything else is unexplained
        # and must not be read as a protocol verdict.
        return completed.returncode, completed.stdout, stderr, _diag(
            DIAG_RUNNER_ERROR, f"sam-conformance exited {completed.returncode} outside its documented codes: {detail}", blocking=True
        )
    return completed.returncode, completed.stdout, stderr, None


def parse_artifact(stdout: str, output: Path) -> tuple[dict[str, Any] | None, str | None]:
    """Prefer the runner's --output file, then its stdout; both must be JSON."""
    if output.is_file():
        try:
            return json.loads(output.read_text(encoding="utf-8")), None
        except (OSError, json.JSONDecodeError) as error:
            return None, f"runner --output file is not valid JSON: {error}"
    text = stdout.strip()
    if not text:
        return None, "runner produced no artifact on stdout or --output"
    try:
        return json.loads(text), None
    except json.JSONDecodeError as error:
        return None, f"runner stdout is not valid JSON: {error}"


def validate_artifact(artifact: dict[str, Any], path: Path | None) -> tuple[bool, str, str]:
    """Validate via the repository checker when present, else a minimal structural check."""
    if path is not None and ARTIFACT_CHECKER.is_file():
        try:
            completed = subprocess.run(
                [sys.executable, str(ARTIFACT_CHECKER), str(path)],
                capture_output=True,
                text=True,
                timeout=60.0,
                check=False,
            )
        except (OSError, subprocess.SubprocessError) as error:
            return False, str(ARTIFACT_CHECKER), f"checker could not run: {type(error).__name__}: {error}"
        if completed.returncode != 0:
            return False, str(ARTIFACT_CHECKER), f"exit {completed.returncode}: {(completed.stderr or completed.stdout).strip()[:400]}"
        return True, str(ARTIFACT_CHECKER), "schema check passed"
    problems: list[str] = []
    if artifact.get("schema_version") != SCHEMA_VERSION:
        problems.append(f"schema_version is {artifact.get('schema_version')!r}, expected {SCHEMA_VERSION!r}")
    rows = artifact.get("rows")
    if not isinstance(rows, list) or not rows:
        problems.append("rows must be a non-empty list")
        rows = rows if isinstance(rows, list) else []
    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            problems.append(f"row {index} is not an object")
            continue
        if not isinstance(row.get("feature"), str):
            problems.append(f"row {index} has no string feature")
        if row.get("result") not in VALID_RESULTS:
            problems.append(f"row {index} has result {row.get('result')!r}, expected one of {VALID_RESULTS}")
    summary = artifact.get("summary")
    if not isinstance(summary, dict):
        problems.append("summary must be an object")
    return (not problems), "builtin-minimal-check", "; ".join(problems) if problems else "minimal structural check passed"


def synthesize_not_run(router: Router, endpoint: str, plan: str, diagnostic: dict[str, str], detail: str) -> dict[str, Any]:
    """A truthful artifact for a lane that could not start: every requested feature is not_run."""
    rows = []
    for feature, dialect in PLAN_FEATURES[plan]:
        operations = (
            (
                "shared_subsession_stream_payload_single_destination",
                "shared_subsession_datagram_payload_single_destination",
            )
            if dialect
            else (f"{feature}_payload_exchange",)
        )
        for operation in operations:
            rows.append({
                "feature": feature,
                "operation": operation,
                "shared_dialect": dialect,
                "local_identity": None,
                "peer_identity": None,
                "result": "not_run",
                # Evidence fields stay present and zeroed: the schema requires them so a
                # not-run row can never be mistaken for an exchange with unknown volume.
                "evidence": {
                    "bytes_sent": 0,
                    "bytes_received": 0,
                    "exact_payload_match": False,
                    "identity_proven": False,
                },
                "diagnostic_category": diagnostic["category"],
                "notes": detail,
            })
    counts = {"pass": 0, "unsupported": 0, "fail": 0, "not_run": len(rows), "create_only": 0, "capability_passes": 0}
    return {
        "schema_version": SCHEMA_VERSION,
        "router": router.label,
        "router_version_or_sha": router.pinned_revision,
        "endpoint": endpoint,
        "peer_endpoint": None,
        "peer_router": None,
        "peer_router_version_or_sha": None,
        "negotiated_sam_version": None,
        "local_identity": None,
        "capabilities": {name: "unknown" for name in (
            "stream",
            "datagram",
            "raw",
            "datagram2",
            "datagram3",
            "shared_primary",
            "shared_master",
            "datagram_direct",
            "raw_direct",
            "session_identity_lookup",
        )},
        "generated_at": now_rfc3339(),
        "plan": plan,
        "rows": rows,
        "summary": counts,
    }


# --------------------------------------------------------------------------
# output
# --------------------------------------------------------------------------


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=False) + "\n", encoding="utf-8")


def write_matrix(path: Path, outcomes: Iterable[Outcome]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(MATRIX_HEADER)
        for outcome in outcomes:
            for row in outcome.rows:
                identity = row.get("local_identity") or {}
                writer.writerow(
                    [
                        outcome.router.label,
                        outcome.artifact.get("router_version_or_sha") or outcome.router.pinned_revision,
                        outcome.artifact.get("negotiated_sam_version") or "not observed",
                        row.get("feature"),
                        row.get("operation"),
                        row.get("shared_dialect") or "not applicable",
                        row.get("result"),
                        row.get("diagnostic_category") or "",
                        (identity.get("destination_hash") if isinstance(identity, dict) else None) or "",
                        (row.get("notes") or "").replace("\n", " "),
                    ]
                )


def print_summary(outcomes: list[Outcome], matrix_path: Path) -> None:
    print("")
    print("interop qualification summary")
    print("===========================")
    for outcome in outcomes:
        summary = outcome.artifact.get("summary") or {}
        blocking = outcome.blocking_diagnostic()
        status = f"blocked: {blocking}" if blocking else ("invalid artifact" if not outcome.artifact_valid else "ran")
        print(f"- {outcome.router.label} @ {outcome.endpoint} [{status}]")
        print(
            "  rows: "
            + ", ".join(f"{key}={summary.get(key, 'n/a')}" for key in ("pass", "unsupported", "fail", "not_run", "create_only"))
        )
        print(f"  negotiated SAM version: {outcome.artifact.get('negotiated_sam_version') or 'not observed'}")
        print(f"  pinned revision: {outcome.router.pinned_revision}")
        print(f"  artifact: {outcome.artifact_path or 'not written'}")
        print(f"  validator: {outcome.validator}")
        for entry in outcome.diagnostics:
            marker = "BLOCKING" if entry.get("blocking") == "true" else "info"
            print(f"  [{entry.get('category')}] ({marker}) {entry.get('detail')}")
        if outcome.runner_exit is not None:
            print(f"  runner exit: {outcome.runner_exit}")
        if not outcome.artifact_valid:
            print("  NOTE: no live payload result is claimed for this router.")
    print("")
    print(f"matrix: {matrix_path}")


def qualification_exit_code(outcomes: list[Outcome]) -> int:
    """Hard failure/invalid artifact outranks a blocked lane; otherwise 3 means blocked."""
    if any(not outcome.artifact_valid or outcome.has_fail_rows() for outcome in outcomes):
        return 1
    if any(outcome.blocking_diagnostic() is not None for outcome in outcomes):
        return 3
    return 0


# --------------------------------------------------------------------------
# orchestration
# --------------------------------------------------------------------------


def qualify_one(router: Router, args: argparse.Namespace) -> Outcome:
    endpoint = args.endpoint or router.bridge_endpoint
    artifact_path = Path(args.artifact_dir) / f"{slug(router.label)}-conformance.json"
    stdout_log = Path(args.artifact_dir) / f"{slug(router.label)}-runner.stdout.log"
    stderr_log = Path(args.artifact_dir) / f"{slug(router.label)}-runner.stderr.log"
    diagnostics: list[dict[str, str]] = []

    # Prerequisite probes run in a fixed order and the first blocking result
    # stops the lane, so a run always reports the root cause rather than a
    # downstream symptom.
    probes: list[Callable[[], dict[str, str]]] = [partial(probe_binary, router)]
    if not args.skip_udp_probe:
        probes.append(partial(probe_udp_egress))
    probes.append(partial(probe_tcp_port, endpoint, args.timeout))
    probes.append(partial(probe_bridge, endpoint, args.timeout))

    blocking: dict[str, str] | None = None
    for probe in probes:
        result = probe()
        diagnostics.append(result)
        if result.get("blocking") == "true":
            blocking = result
            break

    runner_exit: int | None = None
    runner_stderr = ""
    artifact: dict[str, Any] | None = None
    if blocking is None:
        artifact_path.parent.mkdir(parents=True, exist_ok=True)
        # A stale --output file from an earlier run must never be mistaken for
        # this run's artifact when the runner fails before writing one.
        if artifact_path.exists():
            artifact_path.unlink()
        runner_exit, stdout, runner_stderr, diag = run_runner(
            router, endpoint, args.plan, artifact_path, args.timeout, args.peer_endpoint,
            getattr(args, "service_destination", None),
            getattr(args, "control_timeout", 120),
            getattr(args, "datagram_endpoint", None),
        )
        stdout_log.write_text(stdout, encoding="utf-8")
        stderr_log.write_text(runner_stderr, encoding="utf-8")
        if diag is not None:
            diagnostics.append(diag)
            blocking = diag
        else:
            artifact, parse_error = parse_artifact(stdout, artifact_path)
            if artifact is None:
                diagnostics.append(_diag(DIAG_ARTIFACT_ABSENT, parse_error or "no artifact", blocking=True))
                blocking = diagnostics[-1]

    if artifact is None:
        assert blocking is not None  # one of the two branches above always sets it
        detail = f"{blocking['category']}: {blocking['detail']}"
        artifact = synthesize_not_run(router, endpoint, args.plan, blocking, detail)
        write_json(artifact_path, artifact)

    valid, validator, validation_detail = validate_artifact(artifact, artifact_path)
    if not valid:
        diagnostics.append(_diag(DIAG_ARTIFACT_INVALID, validation_detail, blocking=True))
    elif blocking is None:
        diagnostics.append(_diag(DIAG_ARTIFACT_INVALID, validation_detail))

    outcome = Outcome(
        router=router,
        endpoint=endpoint,
        plan=args.plan,
        artifact=artifact,
        artifact_path=artifact_path,
        artifact_valid=valid,
        validator=validator,
        diagnostics=diagnostics,
        runner_exit=runner_exit,
        runner_stderr=runner_stderr,
    )
    return outcome


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--router", action="append", default=[], metavar="LABEL", help="router label from routers.json (repeatable)")
    parser.add_argument("--all", action="store_true", help="qualify every router in routers.json")
    parser.add_argument("--endpoint", default=None, help="override the router's SAM bridge endpoint (HOST:PORT)")
    parser.add_argument(
        "--peer-endpoint",
        default=None,
        help="SAM bridge endpoint used as the generated peer for bidirectional payload rows",
    )
    parser.add_argument(
        "--service-destination",
        default=None,
        help="known I2P hostname or .b32.i2p address; STREAM plan sends GET / and validates HTTP 2xx",
    )
    parser.add_argument("--plan", default=None, choices=sorted(PLAN_FEATURES), help="lane plan (default: routers.json default_plan)")
    parser.add_argument("--artifact-dir", default="artifacts/interop", help="directory for artifacts and logs (default: artifacts/interop)")
    parser.add_argument("--matrix-out", default="artifacts/interop/interop-matrix.csv", help="merged CSV matrix path")
    parser.add_argument("--skip-udp-probe", action="store_true", help="skip the advisory UDP egress probe")
    parser.add_argument("--json-summary", default=None, metavar="PATH", help="also write a machine-readable run summary")
    parser.add_argument("--timeout", type=float, default=300.0, help="per-router runner timeout in seconds (default: 300)")
    parser.add_argument(
        "--control-timeout",
        type=int,
        default=120,
        help="per-SAM-command timeout in seconds (default: 120; SESSION CREATE may need a minute or longer)",
    )
    parser.add_argument(
        "--datagram-endpoint",
        default=None,
        help="override the SAM UDP forwarding endpoint (HOST:PORT); default is the bridge address with TCP port minus one",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.plan is None:
        args.plan = str(json.loads(ROUTERS_CONFIG.read_text(encoding="utf-8")).get("default_plan", "full"))
    if args.plan not in PLAN_FEATURES:
        print(f"interop: unknown plan {args.plan!r}", file=sys.stderr)
        return 2
    if args.timeout <= 0:
        print("interop: --timeout must be positive", file=sys.stderr)
        return 2
    if args.control_timeout <= 0:
        print("interop: --control-timeout must be positive", file=sys.stderr)
        return 2

    routers = load_routers(ROUTERS_CONFIG)
    selected = resolve_routers(routers, args.router, args.all)
    Path(args.artifact_dir).mkdir(parents=True, exist_ok=True)

    outcomes = [qualify_one(router, args) for router in selected]
    matrix_path = Path(args.matrix_out)
    write_matrix(matrix_path, outcomes)

    if args.json_summary:
        write_json(
            Path(args.json_summary),
            {
                "generated_at": now_rfc3339(),
                "plan": args.plan,
                "artifact_dir": str(args.artifact_dir),
                "matrix": str(matrix_path),
                "routers": [
                    {
                        "label": o.router.label,
                        "endpoint": o.endpoint,
                        "pinned_revision": o.router.pinned_revision,
                        "negotiated_sam_version": o.artifact.get("negotiated_sam_version"),
                        "artifact": str(o.artifact_path),
                        "artifact_valid": o.artifact_valid,
                        "validator": o.validator,
                        "runner_exit": o.runner_exit,
                        "blocking_diagnostic": o.blocking_diagnostic(),
                        "diagnostics": o.diagnostics,
                        "summary": o.artifact.get("summary"),
                    }
                    for o in outcomes
                ],
            },
        )

    print_summary(outcomes, matrix_path)

    return qualification_exit_code(outcomes)


if __name__ == "__main__":
    sys.exit(main())
