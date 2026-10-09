# C and Python bindings

These pre-1.0 bindings call the existing blocking Rust client. They do not implement SAM
or router policy independently. The initial surface supports connecting to a bridge,
looking up a name, generating a Destination, and releasing the client.

## C

Build the library with `cargo build -p i2pr-sam-ffi --release`. Include
`crates/i2pr-sam-ffi/include/i2pr_sam.h` and link the resulting `i2pr_sam_ffi` library.

Handles are opaque `uint64_t` values. A successful connect creates one handle; call
`i2pr_sam_close` once to release it. Inputs are UTF-8 byte spans capped at 64 KiB. Output
buffers belong to the caller; lookup and Destination generation set required lengths and
return status 4 when buffers are too small. Return codes are 0 (success), 1 (invalid input),
2 (unknown handle or SAM/runtime failure), 3 (missing required output pointer), 4 (buffer too
small), and 255 (contained Rust panic). Private key material is returned only by the
explicit Destination-generation function and must be handled as a secret.

## Python

Build/install locally with `maturin develop --manifest-path
crates/i2pr-sam-python/Cargo.toml` from an activated Python environment with maturin
installed. The module is named `i2pr_sam`:

```python
import i2pr_sam

client = i2pr_sam.Client("127.0.0.1:7656")
value = client.lookup("example.i2p")
public_destination, private_destination = client.generate_destination()
```

The generated private Destination is returned only from the explicit generation call.
Keep it out of logs and persistent storage unless the application intentionally manages
that identity. Python methods are synchronous and should not be called from a Tokio runtime
thread.

No crate or wheel publication is configured. Distribution requires a separate license and
release decision.
