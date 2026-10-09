"""Synthetic fixture corpus for the repository guard self-tests.

The two repository guards (`scripts/check-api-snapshot.py` and
`scripts/check-proto-boundary.py`) are standalone, stdlib-only scripts so they
can be invoked directly from CI as `python3 scripts/<guard>.py`. They locate
this directory relative to their own path instead of importing it, which keeps
each guard readable on its own and keeps the fixtures independent of the guard
code that consumes them.

Contents:

- `mutations/api/`: one realistic synthetic Rust `lib.rs` baseline plus one file
  per known public-surface breakage. Every mutation shares `base.rs` so the
  "unchanged source" negative control is meaningful.
- `mutations/boundary/`: one clean manifest (negative control) plus manifests
  carrying each forbidden runtime/network dependency form.

None of these files are compiled, published, or read by the ordinary (non
self-test) guard run. A self-test copies what it needs into a
`tempfile.mkdtemp()` sandbox first, so the guards never read or write a checked
in repository artifact while proving themselves.
"""