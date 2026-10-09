"""Router-enabled interoperability qualification harness.

This package holds infrastructure only: declarative router pins, a standalone
UDP egress probe, and a qualification driver that consumes the JSON artifact
emitted by the repository conformance runner. No router implementation source is
copied here; the routers are treated as black-box peers.
"""