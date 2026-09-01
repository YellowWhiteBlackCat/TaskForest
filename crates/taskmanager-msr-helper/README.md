# taskmanager-msr-helper

## Role

One-shot Linux helper for CPU MSR readouts (package temperature, P-state
multipliers, P-state core voltage on Intel; P-state multipliers and SVI2
Vcore on AMD Zen-family parts), invoked through the escalation seam and
emitting a typed JSON envelope.

## Boundary

The helper performs exactly one operation: enumerate the existing
`/dev/cpu/N/msr` nodes (sorted by `N`, capped at 1024) and `pread` the
register set selected by the CPUID family gate — the five verified Intel
registers (`0xCE` PLATFORM_INFO, `0x1AD` TURBO_RATIO_LIMIT, `0x198`
IA32_PERF_STATUS, `0x1A2` TEMPERATURE_TARGET, `0x1B1`
PACKAGE_THERM_STATUS), or, for CPUID family 0x17–0x19, the AMD P-state
block of ADR-049 (`0xC0010063` status + `0xC0010064..0xC001006B`). MSR
reads on Linux are plain file I/O (open + pread at the register-address
offset — the same mechanism libcpuid uses for CPU-X), so this helper is
`#![forbid(unsafe_code)]` std + serde with zero workspace dependencies: no
fifth audited unsafe trust root (ADR-048). The same is true of the
read-only `/dev/cpu/N/cpuid` sibling node (one 16-byte pread per leaf; the
driver registers no write): it supplies the family gate and the base-clock
enumeration. No flags, no ioctl, no file writes, no other path.

A register the CPU does not implement (driver `EIO`) or a value outside its
documented plausibility range becomes a per-field `null` — the vendor gate
is per-register honesty, not a CPUID dependency. `bclk_mhz` is filled only
when CPUID leaf 0x16 enumerates the SDM Bus (Reference) Frequency inside
the 20–500 MHz envelope (ADR-048 amendment); AMD temperature and BCLK stay
`null` — temperature already reaches the product unprivileged through the
k10temp/zenpower hwmon chips, and SMN-only telemetry needs PCI-config
writes or `mmap` (ADR-049 records why that is structurally out of reach).

## Contract and verification

Exit codes (0 success; 2 permission_denied, 3 no_msr, 4 open_failed, 5
read_failed), the field-disjoint SUCCESS/ERROR envelopes, the pure decode
tables (Intel and AMD), node sorting/capping, honest-null semantics, the
CPUID identity gates, and denial classification are covered by headless
fixture tests (the `/dev/cpu` root is a parameter, so tests run against
fixture trees, never the live host; the AMD P-state block fixture is a
sparse file because the block's registers sit at consecutive MSR
addresses):

```bash
cargo nextest run -p taskmanager-msr-helper -j 4 --all-targets
cargo clippy -p taskmanager-msr-helper --all-targets -- -D warnings
```

Package/polkit verification and a live on-box pkexec receipt are separate;
without the latter the capability remains permission-limited.

## Module map

```text
src/main.rs → msr_read.rs    pre-read of fixed MSR registers (/dev/cpu/*/msr)
                             and the /dev/cpu/N/cpuid family gate
src/json.rs                  honest-null JSON envelope output
```
