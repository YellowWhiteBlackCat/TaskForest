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
- tracing initialization on the GUI path only;
- the one-shot snapshot collector (`collect_json_snapshot_from_client`,
  including the per-process GPU fdinfo bulk scan in
  [`cli_process_gpu`]).

## Boundary

Shape differences enter as plain values in `run::FrontendHandlers`; the handler
set, the no-cfg/no-frontend-identity rule and the fifth-frontend path are
specified in [`docs/MODES_AND_HANDLERS.md`](docs/MODES_AND_HANDLERS.md).
Dependencies stop at the composition edge: core, application, app-host,
assets, escalation — no toolkit, no OS adapter, no frontend crate.

## Contract and verification

The JSON snapshot honesty contract, the `FrontendHandlers` composition rule
and the verification route are specified in
[`docs/MODES_AND_HANDLERS.md`](docs/MODES_AND_HANDLERS.md).

## Module map

```text
src/cli.rs (+ cli/suggest.rs)              argv parsing, CliMode, typed arg errors
src/run.rs                                 FrontendHandlers composition and entry
src/cli_gpu_engines.rs                     --gpu-engines escalation mode
src/cli_memory_smbios.rs                   --memory-smbios escalation mode
src/cli_msr.rs                             --msr escalation mode
src/cli_package_power.rs                   --package-power escalation mode
src/cli_process_gpu.rs                     per-process GPU fdinfo bulk scan
```

## Docs

- [`docs/MODES_AND_HANDLERS.md`](docs/MODES_AND_HANDLERS.md) — the UI-neutral mode list,
  `FrontendHandlers` shape and the JSON honesty verification.
- [ADR-051](../../adr/051-four-frontend-products-zero-ui-features.md) — four frontend
  products with zero `ui-*` features.
