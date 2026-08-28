# ADR-043: Defer OpenHarmony native telemetry until a mature Rust seam exists

Status: accepted

## Context

`taskmanager-platform-ohos` now provides an independent composition seam, but
it does not yet have a trustworthy CPU or memory provider. The mature
cross-platform `sysinfo` crate publishes a supported-OS list that does not
include OpenHarmony and documents that unsupported systems may return empty
values. Compiling against the Rust `aarch64-unknown-linux-ohos` target is
therefore not evidence that a Linux-shaped `sysinfo` provider observes real
OHOS data.

OpenHarmony exposes HiDebug CPU and memory APIs, but the native route is a C
ABI boundary. The available community `ohos-sys` bindings are explicitly
unofficial and under active development; their component status does not yet
provide a completed `hidebug` binding. The repository's safe-Rust policy does
not permit adding an unreviewed FFI call directly to the platform adapter.

## Decision

1. Do not add `sysinfo` as an OHOS telemetry authority.
2. Do not add an unofficial OHOS binding as a production dependency.
3. Keep CPU and memory telemetry in `taskmanager-platform-ohos` as an
   explicitly deferred capability. The current runtime remains capability
   absent and returns typed unsupported outcomes.
4. Reconsider this decision only when one of these conditions is met:
   - a maintained safe Rust wrapper explicitly supports the target and the
     required API semantics; or
   - the repository accepts a dedicated, audited OHOS native boundary with an
     ADR, SDK/version policy, permission mapping, ABI tests, and target-device
     receipts.
5. A future provider must validate values and freshness on an actual
   OpenHarmony target. Target compilation alone cannot promote the capability
   to `Available`.

## Consequences

The OHOS crate stays small, safe, and honest. No provider dependency or C ABI
surface is added before its trust and maintenance cost is justified. CPU and
memory support may remain absent for an extended period; that is preferable to
publishing empty or Linux-shaped measurements as real telemetry.

The shared core and application contracts remain reusable, and later provider
work can enter through the existing `CpuTelemetryProvider` and
`MemoryTelemetryProvider` seams without changing the domain model.

## Verification

- `taskmanager-platform-ohos` has no UI or Linux-adapter dependency.
- The crate's empty runtime tests prove unsupported requests remain typed.
- The OHOS target check covers the shared core, application, and independent
  adapter seam.

## References

- [`sysinfo` supported OSes](https://docs.rs/sysinfo/latest/sysinfo/)
- [OpenHarmony HiDebug APIs](https://developer.huawei.com/consumer/cn/doc/doccenter-capabilities/hidebug)
- [`ohos-sys` binding status](https://github.com/openharmony-rs/ohos-sys)
