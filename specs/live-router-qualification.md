# Live-router qualification attempt

Attempted 2026-10-07 with the repository conformance runner and pinned revisions from
`references/sam-v3-reference-freeze.md`:

| Router | Invocation | Result |
|---|---|---|
| Java I2P | `rtk cargo run --locked -p i2pr-sam --bin sam-conformance -- --endpoint 127.0.0.1:7656 --router 'Java I2P' --router-version a629ec7c9c675dd252d005fb881efd92e8e6ba27` | Exit 2; connection refused |
| i2pd | `rtk cargo run --locked -p i2pr-sam --bin sam-conformance -- --endpoint 127.0.0.1:7656 --router i2pd --router-version d147bb0fd6789c75dc1c4d70c4f79b151a552d53` | Exit 2; connection refused |
| i2pr | `rtk cargo run --locked -p i2pr-sam --bin sam-conformance -- --endpoint 127.0.0.1:7656 --router i2pr --router-version 4f4e98f5a0241355af5099c73065088dede1b1de` | Exit 2; connection refused |

No live row is a pass. `ss -ltn` showed no listener on ports 7656–7658, `i2pd` was not
installed, no router process was running, and access to the Docker daemon socket was
denied. The repeated commands establish unavailable prerequisites, not router protocol
failures. Repeat these probes in a router-enabled environment before claiming
interoperability.
