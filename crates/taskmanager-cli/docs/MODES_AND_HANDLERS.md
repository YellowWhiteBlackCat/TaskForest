# Modes and frontend handlers

The six UI-neutral modes are listed in the README Role section; this doc owns
their contract detail.

## Honesty contract

The help text and the honest `unsupported` reporting belong to the CLI: a
product that does not carry a capability says so in typed form instead of
fabricating data. The JSON snapshot honesty contract — unavailable domains
serialize as typed null/discriminators, never a fabricated `0` — is proven
end-to-end by `tests/logic/main_tests.rs` against the real native runtime,
and the parser/renderer contracts by the `cli_*_tests` units mounted through
`#[path]`. Tracing initializes on the GUI path only; the `--json` snapshot
mode owns stdout exclusively.

## Frontend handlers

The handler struct carries all shape differences: a
required `run_gui` handler plus optional `snapshot_text` (TUI product) and
`capture_window` (Windows GPUI product). There is no `cfg`, no feature, and
no frontend identity anywhere in this crate; a fifth frontend is a new
product crate plus one handler struct.

## Verification

```bash
cargo nextest run -p taskmanager-cli -j 4
```
