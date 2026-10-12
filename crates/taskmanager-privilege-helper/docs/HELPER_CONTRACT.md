# Helper contract

Each binary target has a separate one-operation boundary. The typed outcome
surface, exit code, JSON schema, denial and malformed input are part of that
target's contract. Package/polkit verification and live hardware receipts are
separate from compilation.

## Modules

- `src/main.rs` — root entry, single bounded invocation.
- `src/discovery.rs` / `src/engine_names.rs` — GPU engine discovery.
- `src/sample.rs` — PMU sampling.
- `src/json.rs` — fixed JSON envelope output.
- `src/smart_self_test_main.rs` / `src/smart_self_test.rs` — SMART self-test
  entry point and fixed identity-checked operation.

The SMART target accepts exactly `--start-self-test DEVICE_ID GENERATION
LOCATOR PROTOCOL INTERCONNECT KIND`. It permits only known storage enum values,
requires a valid lifecycle generation and persistent WWID/serial identity,
recomputes the live identity from sysfs, reads the device's exact per-kind
self-test support, rechecks identity, and then runs one bounded `smartctl -t`
operation. It launches no shell and accepts no caller-supplied executable or
arbitrary option.

Its success contract is `{"schema":1,"status":"started"}`. Its error
contract is `{"schema":1,"status":"error","kind":"..."}` with a
closed `kind` vocabulary parsed by `taskmanager-escalation`.

```bash
cargo nextest run -p taskmanager-privilege-helper -j 4 --all-targets
```
