#!/usr/bin/env python3
"""Standalone UDP egress probe.

An I2P router cannot join the network without outbound UDP, so the interop
harness must distinguish "the router is broken" from "this host has no UDP
egress at all". This script answers only the latter question, which is why it
imports nothing from the harness.

A single send() to an unreachable network proves nothing on its own: a silently
dropped datagram looks identical to a successful send. So a target counts as
reachable only when the host sends a UDP payload and then receives any reply on
the same socket within the timeout.

Three targets are tried and reported separately, because "some UDP works" is not
the same question as "an I2P router can join the network":

* 53/UDP - a real recursive resolver. Often permitted by sandbox policies.
* 443/UDP - QUIC Initial shape. Usually blocked wherever a TCP proxy is used.
* a non-standard high port - the decisive test. I2P peers connect over SSU on
  random high UDP ports, so a host that answers on 53 but drops everything above
  1024 cannot run a live router however healthy its DNS path looks.

Exit codes: 0 I2P-capable UDP egress works, 1 it does not, 2 usage error.
"""
from __future__ import annotations

import argparse
import json
import socket
import sys
from typing import Any

# QUIC Initial packets start with a long-header first byte with the fixed bit
# set, then a version of 0x00000001 and a random-looking DCID. We do not expect
# a valid handshake; we only need a reply of any kind to prove the packet left
# the host and reached a live responder.
QUIC_TARGET = ("1.1.1.1", 443)
DNS_TARGET = ("1.1.1.1", 53)
# I2P SSU peers listen on random high UDP ports; a reply here is what a router
# would actually need.
HIGH_PORT_TARGET = ("1.1.1.1", 34567)


def _quic_initial_payload() -> bytes:
    return b"\xc3" + b"\x00\x00\x00\x01" + bytes(range(16)) + b"\x08" + b"\x00" * 64


def _dns_query_payload() -> bytes:
    header = b"\x2b\x2c" + b"\x01\x00" + b"\x00\x01" + b"\x00\x00" + b"\x00\x00" + b"\x00\x00"
    qname = b"".join(bytes([len(p)]) + p.encode("ascii") for p in ("example", "com"))
    return header + qname + b"\x00" + b"\x00\x01" + b"\x00\x01"


def _attempt(target: tuple[str, int], payload: bytes, timeout: float) -> tuple[bool, str | None]:
    """Send one datagram and wait for any reply; UDP ICMP errors surface as OSError."""
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
    """Report which UDP destinations answer, and whether that is enough for a router."""
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
    high_port_ok = any(entry.startswith("high_port") for entry in reachable)
    any_udp = bool(reachable)
    if high_port_ok:
        verdict = "udp egress reaches non-standard ports; a router can attempt SSU peering"
    elif any_udp:
        verdict = (
            "udp answers on restricted ports only; an I2P router cannot peer over SSU "
            "on random high ports from this host"
        )
    else:
        verdict = "no udp egress at all"
    result: dict[str, Any] = {
        "udp_egress": any_udp,
        "udp_high_port_egress": high_port_ok,
        "i2p_router_udp_capable": high_port_ok,
        "probe_target": "; ".join(f"{t[0]}:{t[1]}" for t, _, _ in attempts),
        "reachable": reachable,
        "verdict": verdict,
        "error": None if any_udp else "; ".join(errors),
    }
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--timeout", type=float, default=5.0, help="per-attempt receive timeout in seconds (default: 5)")
    args = parser.parse_args(argv)
    if args.timeout <= 0:
        print(
            json.dumps(
                {
                    "udp_egress": False,
                    "udp_high_port_egress": False,
                    "i2p_router_udp_capable": False,
                    "probe_target": "none",
                    "reachable": [],
                    "verdict": "invalid timeout",
                    "error": "invalid timeout",
                }
            )
        )
        return 2
    result = probe(args.timeout)
    print(json.dumps(result))
    # Exit status tracks router capability, not merely "some UDP answered".
    return 0 if result["i2p_router_udp_capable"] else 1


if __name__ == "__main__":
    sys.exit(main())