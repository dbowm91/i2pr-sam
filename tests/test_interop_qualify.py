from __future__ import annotations

import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from dataclasses import replace
from types import SimpleNamespace
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "interop"))
import qualify  # noqa: E402


def router() -> qualify.Router:
    return qualify.Router("test", "", "revision", "127.0.0.1:7656", "127.0.0.1:7655", None,
                          "bridge_only", {}, {}, "")


class QualificationHarnessTests(unittest.TestCase):
    def test_runner_command_peer_endpoint_is_optional_and_forwarded_once(self) -> None:
        base = qualify.runner_command(router(), "127.0.0.1:7656", "stream", Path("out.json"))
        self.assertNotIn("--peer-endpoint", base)
        argv = qualify.runner_command(router(), "127.0.0.1:7656", "stream", Path("out.json"), "127.0.0.1:7657")
        self.assertEqual(argv.count("--peer-endpoint"), 1)
        self.assertEqual(argv[argv.index("--peer-endpoint") + 1], "127.0.0.1:7657")

    def test_peer_endpoint_reaches_run_runner_from_qualify_one(self) -> None:
        artifact = {"schema_version": "1.1", "rows": [{"feature": "stream", "result": "not_run"}], "summary": {}}
        with tempfile.TemporaryDirectory() as artifact_dir:
            args = SimpleNamespace(endpoint="127.0.0.1:7656", artifact_dir=artifact_dir, plan="stream",
                                   timeout=1.0, peer_endpoint="127.0.0.1:7657", skip_udp_probe=False)
            probes = [patch.object(qualify, "probe_binary", return_value=qualify._diag("binary", "ok")),
                      patch.object(qualify, "probe_udp_egress", return_value=qualify._diag("udp", "unknown")),
                      patch.object(qualify, "probe_tcp_port", return_value=qualify._diag("port", "open")),
                      patch.object(qualify, "probe_bridge", return_value=qualify._diag("bridge", "hello")),
                      patch.object(qualify, "validate_artifact", return_value=(True, "test", "valid"))]
            for probe in probes:
                probe.start()
            try:
                with patch.object(qualify, "run_runner", return_value=(0, __import__("json").dumps(artifact), "", None)) as run:
                    outcome = qualify.qualify_one(router(), args)
                self.assertEqual(run.call_args.args[-1], "127.0.0.1:7657")
                self.assertTrue(outcome.artifact_valid)
            finally:
                for probe in reversed(probes):
                    probe.stop()

    def test_workflow_targets_provisioned_self_host_and_passes_peer(self) -> None:
        workflow = (ROOT / ".github/workflows/live-interop.yml").read_text()
        self.assertIn("runs-on: [self-hosted, linux, x64, i2p-interop]", workflow)
        self.assertIn('if [ -n "$PEER_ENDPOINT" ]; then', workflow)
        self.assertIn('--peer-endpoint "$PEER_ENDPOINT"', workflow)

    def test_udp_arbitrary_silence_is_advisory_unknown(self) -> None:
        completed = subprocess.CompletedProcess([], 1, '{"reachability":"unknown","reachable":[],"verdict":"unknown"}', "")
        with patch.object(qualify.subprocess, "run", return_value=completed):
            result = qualify.probe_udp_egress()
        self.assertEqual(result["blocking"], "false")
        self.assertIn("unknown", result["detail"])

    def test_udp_tristate_requires_positive_evidence_for_unreachable(self) -> None:
        self.assertEqual(qualify.udp_reachability({"reachability": "reachable"}), "reachable")
        self.assertEqual(qualify.udp_reachability({"reachability": "unknown"}), "unknown")
        self.assertEqual(qualify.udp_reachability({"reachability": "unreachable"}), "unknown")
        self.assertEqual(qualify.udp_reachability({"reachability": "unreachable", "unreachable_evidence": ["icmp administratively prohibited"]}), "unreachable")

    def test_udp_probe_positive_reply_is_advisory_reachable(self) -> None:
        completed = subprocess.CompletedProcess([], 0, '{"reachability":"reachable","reachable":["dns_53"]}', "")
        with patch.object(qualify.subprocess, "run", return_value=completed):
            result = qualify.probe_udp_egress()
        self.assertEqual(result["blocking"], "false")
        self.assertIn("request/reply observed", result["detail"])

    def test_bridge_hello_success_and_unavailable(self) -> None:
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        endpoint = f"127.0.0.1:{listener.getsockname()[1]}"

        def serve() -> None:
            conn, _ = listener.accept()
            with conn:
                conn.recv(512)
                conn.sendall(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
            listener.close()

        thread = threading.Thread(target=serve)
        thread.start()
        self.assertEqual(qualify.probe_bridge(endpoint, 1)["blocking"], "false")
        thread.join(timeout=2)
        self.assertFalse(thread.is_alive())
        self.assertEqual(qualify.probe_bridge("127.0.0.1:1", 0.1)["blocking"], "true")

    def test_missing_binary_blocks_non_bridge_only_router(self) -> None:
        candidate = replace(router(), binary="definitely-not-a-router-binary", binary_probe="path")
        self.assertEqual(qualify.probe_binary(candidate)["blocking"], "true")

    def test_invalid_artifact_outranks_blocked_lane(self) -> None:
        blocked = qualify.Outcome(router(), "endpoint", "stream", {"rows": []}, None, True, "test",
                                  diagnostics=[qualify._diag("blocked", "missing", blocking=True)])
        invalid = qualify.Outcome(router(), "endpoint", "stream", {"rows": []}, None, False, "test")
        self.assertEqual(qualify.qualification_exit_code([blocked]), 3)
        self.assertEqual(qualify.qualification_exit_code([blocked, invalid]), 1)
        self.assertEqual(qualify.qualification_exit_code([]), 0)

    def test_not_run_artifact_synthesis_accounts_for_requested_rows(self) -> None:
        artifact = qualify.synthesize_not_run(router(), "endpoint", "stream", qualify._diag("blocked", "detail"), "detail")
        self.assertEqual([row["result"] for row in artifact["rows"]], ["not_run"])
        self.assertEqual(artifact["summary"]["capability_passes"], 0)

    def test_python_cache_artifacts_are_not_tracked(self) -> None:
        tracked = subprocess.run(["git", "ls-files"], cwd=ROOT, check=True, capture_output=True, text=True).stdout.splitlines()
        self.assertFalse([path for path in tracked if "__pycache__/" in path or path.endswith((".pyc", ".pyo"))])
        ignore = (ROOT / ".gitignore").read_text()
        self.assertIn("__pycache__/", ignore)
        self.assertIn("*.py[cod]", ignore)


if __name__ == "__main__":
    unittest.main()
