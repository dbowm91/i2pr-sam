#!/usr/bin/env python3
"""Advisory UDP reachability probe.

Replies prove that at least one UDP request/reply path works. Silence from an
arbitrary public endpoint is inconclusive: the endpoint may not implement the
sent protocol, or a firewall may drop the reply. This probe never claims UDP
egress is blocked and never gates router qualification.

Exit codes: 0 at least one target replied, 1 no target replied (unknown),
2 usage error.
"""
from __future__ import annotations

import argparse
import json
import socket
import sys
from typing import Any

QUIC_TARGET = ("1.1.1.1", 443)
DNS_TARGET = ("1.1.1.1", 53)
HIGH_PORT_TARGET = ("1.1.1.1", 34567)


def _quic_initial_payload() -> bytes:
    return b"\xc3" + b"\x00\x00\x00\x01" + bytes(range(16)) + b"\x08" + b"\x00" * 64


def _dns_query_payload() -> bytes:
    header = b"\x2b\x2c" + b"\x01\x00" + b"\x00\x01" + b"\x00\x00" + b"\x00\x00" + b"\x00\x00"
    qname = b"".join(bytes([len(p)]) + p.encode("ascii") for p in ("example", "com"))
    return header + qname + b"\x00" + b"\x00\x01" + b"\x00\x01"


def _attempt(target: tuple[str, int], payload: bytes, timeout: float) -> tuple[bool, str | None]:
    """Send one datagram and wait for any reply; silence is not a negative verdict."""
    family = socket.AF_INET6 if ":" in target[0] else socket.AF_INET
    sock = socket.socket(family, socket.SOCK_DGRAM)
    try:
        sock.settimeout(timeout)
        sock.sendto(payload, target)
        data, _peer = sock.recvfrom(2048)
        return True, None if data else "empty UDP reply"
    except socket.timeout:
        return False, f"no UDP reply from {target[0]}:{target[1]} within {timeout:g}s"
    except OSError as error:
        return False, f"{type(error).__name__}: {error}"
    finally:
        sock.close()


def probe(timeout: float) -> dict[str, Any]:
    """Report positive replies and preserve all no-reply results as unknown."""
    attempts = [
        (DNS_TARGET, _dns_query_payload(), "dns_53"),
        (QUIC_TARGET, _quic_initial_payload(), "quic_initial_443"),
        (HIGH_PORT_TARGET, _quic_initial_payload(), "high_port"),
    ]
    reachable: list[str] = []
    errors: list[str] = []
    for target, payload, name in attempts:
        ok, error = _attempt(target, payload, timeout)
        if ok:
            reachable.append(f"{name} at {target[0]}:{target[1]}")
        else:
            errors.append(error or f"{name} failed without a reported error")
    state = "reachable" if reachable else "unknown"
    return {
        "udp_egress": bool(reachable),
        "udp_high_port_egress": None,
        "i2p_router_udp_capable": None,
        "reachability": state,
        "probe_target": "; ".join(f"{target[0]}:{target[1]}" for target, _, _ in attempts),
        "reachable": reachable,
        "verdict": state,
        "error": None if reachable else "; ".join(errors),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--timeout", type=float, default=5.0, help="per-attempt receive timeout in seconds (default: 5)")
    args = parser.parse_args(argv)
    if args.timeout <= 0:
        print(json.dumps({"reachability": "unknown", "reachable": [], "verdict": "invalid timeout", "error": "invalid timeout"}))
        return 2
    result = probe(args.timeout)
    print(json.dumps(result))
    return 0 if result["reachability"] == "reachable" else 1


if __name__ == "__main__":
    sys.exit(main())
