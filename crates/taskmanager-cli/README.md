# taskmanager-cli

## Role

Shared CLI composition harness for the four frontend products (ADR-051).
Every product `[[bin]]` (`taskforest-g`, `taskforest-i`, `taskmanager-tui`,
`taskforest-b`) is a thin shim that calls [`taskmanager_cli::run`] with its
own binary name and capability handlers. This crate owns:

- argv parsing ([`cli::parse_args`], the `CliMode` enum and typed arg errors);
- the UI-neutral modes: `--json`, `--suggest-thresholds`,
  `--gpu-engines`, `--memory-smbios`, `--package-power`, `--msr` (the
  escalation modes drive the ADR-023 polkit/pkexec helpers on demand);
- the help text and the honest `unsupported` reporting for capabilities a
  product does not carry;
- tracing initialization on the GUI path only (the JSON snapshot mode owns
  stdout exclusively);
- the one-shot snapshot collector (`collect_json_snapshot_from_client`,
  including the per-process GPU fdinfo bulk scan in
  [`cli_process_gpu`]).

## Boundary

Shape differences enter as plain values in
[`run::FrontendHandlers`] — a required `run_gui` handler plus optional
`snapshot_text` (TUI product) and `capture_window` (Windows GPUI product).
There is no `cfg`, no feature, and no frontend identity anywhere in this
crate; a fifth frontend is a new product crate plus one handler struct.

Dependencies stop at the composition edge: core, application, app-host,
assets, escalation. No toolkit, no OS adapter, no frontend crate.

## Contract and verification

The JSON snapshot honesty contract (unavailable domains serialize as typed
null/discriminators, never a fabricated `0`) is proven end-to-end by
`tests/logic/main_tests.rs` against the real native runtime, and the parser/
renderer contracts by the `cli_*_tests` units mounted through `#[path]`.
Run: `cargo nextest run -p taskmanager-cli -j 4`.
