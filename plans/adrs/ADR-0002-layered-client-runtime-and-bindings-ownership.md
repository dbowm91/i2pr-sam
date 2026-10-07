# ADR-0002 — Layered client, runtime, bindings, and daemon ownership

Status: accepted.

## Context

The project must serve Rust applications directly, later expose blocking/C/Python
surfaces, and eventually drive a tunnel daemon. Combining all of those concerns in one
crate would make protocol testing, runtime ownership, and API stability difficult.

## Decision

Use a layered workspace:

```text
sam protocol/state
      |
      v
canonical async Rust client
      |
      +--> blocking facade
      +--> C ABI
      +--> Python binding
      |
      +--> service-tunnel adapter
              |
              v
        standalone daemon
              |
              +--> CLI
              +--> WebUI
```

The protocol/state layer owns parsing, serialization, bounded values, typed replies, and
state-machine legality. It owns no sockets, timers, tasks, filesystem, or process state.

The canonical async Rust client owns SAM connections/session lifecycle. Tokio is the
initial production runtime unless an implementation milestone demonstrates a simpler
executor-neutral approach without infecting the protocol layer. Public domain semantics
must not depend on global Tokio state.

The blocking/C/Python layers wrap the canonical Rust behavior rather than reimplementing
SAM.

The future service-tunnel adapter consumes the public `i2pr-service-tunnels` policy
crate. The daemon owns local sockets, process/config/persistence/reload/reconnect and
management lifecycle.

## Consequences

- Bindings wait until the Rust API is exercised and reviewed.
- A daemon feature does not justify moving filesystem/process ownership into the client.
- Runtime abstraction is added only when demanded by a real consumer, not preemptively.
- The project may begin with fewer crates than the final graph, but dependency direction
  may not reverse.
