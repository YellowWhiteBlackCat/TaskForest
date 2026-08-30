# ADR-048: MSR readout helper as a safe file-I/O crossing

Status: accepted

- Relates to: [ADR-023 (per-feature privilege-escalation framework)](023-per-feature-privilege-escalation-framework.md),
  [permission model Boundary 2/3](../docs/PERMISSION_MODEL.md), and
  [ADR-049 (AMD MSR readouts within the safe file-I/O crossing)](049-amd-msr-readouts-safe-file-io.md).

## Context

CPU package temperature, P-state multipliers and the P-state core voltage live
in model-specific registers (MSRs). On Linux the `msr` driver exports them as
`/dev/cpu/<N>/msr` character nodes, mode 0600 root-only — the escalation column
of Boundary 3. The key mechanical fact (verified in CPU-X's actual MSR engine,
which is libcpuid's `rdmsr.c`, CPU-X 4.x delegates all MSR work there): reading
a register is `open("/dev/cpu/%u/msr", O_RDONLY)` followed by one `pread` of 8
bytes at the register-address offset. No `ioctl`, no syscall wrapper, no
pointer plumbing. The FreeBSD path needs a `cpuctl` ioctl; the Linux path this
helper implements is plain file I/O. libcpuid's Linux driver:
`snprintf(msr, MSR_PATH_LEN, "/dev/cpu/%u/msr", core_num)`, `open(msr,
O_RDONLY)`, `ret = pread(driver->fd, result, 8, msr_index)`.

That fact decides the trust question: an MSR readout helper needs **no fifth
audited `unsafe` boundary crate**. Boundary 1 keeps exactly its four crates;
`taskmanager-msr-helper` is `#![forbid(unsafe_code)]` std + serde file I/O,
shaped exactly like the SMBIOS (ADR-023 C1) and RAPL helpers.

### Verified register semantics

Each decoded field was cross-checked between libcpuid `rdmsr.c` (what CPU-X
links), the Linux kernel's own consumers, and the Intel SDM before being
admitted:

1. **Package temperature** — `MSR_TEMPERATURE_TARGET` 0x1A2 bits 23:16 hold
   TjMax; `IA32_PACKAGE_THERM_STATUS` 0x1B1 bits 23:16 hold the Pkg Digital
   Readout with bit 31 the readout-valid gate; temperature = TjMax − readout.
   Kernel `drivers/thermal/intel/intel_tcc.c`: TjMax `"(msrval.l >> 16) &
   0xff"` with `return val ? val : -ENODATA`; validity `if (!(val.l &
   BIT(31))) return -ENODATA`; result `tjmax - ((val.l >> 16) & mask)`.
   `drivers/hwmon/coretemp.c` reads the same layout for both the per-core
   (0x19C) and package (0x1B1) registers.
2. **Current multiplier** — `IA32_PERF_STATUS` 0x198 bits 15:0, the SDM's
   "Current Performance Ratio Value".
3. **Multiplier minimum** — `MSR_PLATFORM_INFO` 0xCE bits 47:40 (Maximum
   Efficiency Ratio); libcpuid `get_info_min_multiplier` reads exactly 47..40.
4. **Multiplier maximum** — `MSR_TURBO_RATIO_LIMIT` 0x1AD bits 7:0 (maximum
   1-core turbo ratio); libcpuid `get_info_max_multiplier`, SDM, kernel
   `msr-index.h` (`MSR_TURBO_RATIO_LIMIT 0x1AD`).
5. **Vcore** — `IA32_PERF_STATUS` 0x198 bits 47:32 divided by 2^13 volts
   (SDM Vol 3: "P-state core voltage can be computed by MSR_PERF_STATUS[37:32]
   * (float) 1/(2^13)"; libcpuid identical). Modern Intel returns 0 in this
   field, which decodes to an honest `null`, never 0 V.

### Excluded readouts (recorded, not guessed)

- **AMD SMU readouts** (Vcore/Tctl via SMU mailboxes) moved to
  [ADR-049](049-amd-msr-readouts-safe-file-io.md): the P-state-register
  readouts that ARE plain RDMSR territory shipped there; SMN-only telemetry
  is structurally unreachable from a no-writes safe helper.

### Base clock (BCLK) — amendment: the rejected avenues and the shipped one

The original decision kept `bclk_mhz` `null` because 0xAD/0xEE carry no base
clock and libcpuid's derivation needs an `rdtsc` timing loop. The follow-up
research closed the question:

1. **`base_frequency` ÷ MSR ratio — REJECTED.** The kernel's semantics
   (`intel_pstate.c` `show_base_frequency`) are
   `rounddown(guaranteed_perf × pstate.scaling, perf_ctl_scaling)`, and
   `pstate.scaling` is NOT the BCLK on hybrid parts: it is
   100000 kHz only for non-hybrid cores and E-cores; hybrid P-cores use a
   fixed factor (78741/80000/86957 kHz) or the CPPC-derived
   `nominal_freq × 1000 / nominal_perf`. On the reference host (Arrow
   Lake-H): scaling = 1900×1000/21 = 90476 kHz and `base_frequency` =
   1900000 = `rounddown(22 × 90476, 100000)` — dividing by either ratio
   fabricates a figure (nominal/max-efficiency division would yield
   86.36/475 MHz where the true BCLK is 100, both inside a naive
   20–500 envelope).
2. **TSC timing ÷ `MSR_PLATFORM_INFO[15:8]` (libcpuid `get_info_bus_clock`)
   — REJECTED.** `_rdtsc` is an unsafe intrinsic, unavailable under
   `#![forbid(unsafe_code)]`; and the premise "TSC rate = non-turbo ratio ×
   BCLK" is false on modern parts — on the reference host the TSC runs at
   3686.4 MHz (CPUID 0x15: 96 × 38.4 MHz crystal) against a 37 × 100 MHz
   base, so the formula computes 99.63 MHz. Model-dependent without a
   verifier.
3. **CPUID leaf 0x16 ECX — SHIPPED.** The SDM defines the leaf's ECX
   bits 15:0 as the Bus (Reference) Frequency in MHz, and the kernel itself
   consumes the leaf (`tsc.c` `native_calibrate_tsc`,
   `cpu_khz_from_cpuid`). Read from the read-only `/dev/cpu/N/cpuid` node
   (plain 16-byte pread; the driver has no `.write`). On the reference host
   it enumerates 100 MHz, consistent with every other frequency figure
   (base 3700 = 37×100, max turbo 4700000 kHz = 47×100, min 400000 kHz =
   4×100, and intel_pstate's hardcoded 100000 kHz/ratio for the family).
   Honesty gates: the CPU's max standard leaf must reach 0x16 (an
   out-of-range leaf aliases to leaf 0, i.e. vendor ASCII in ECX), the
   field must be non-zero, and the value must sit in the 20–500 MHz
   envelope; otherwise the field stays `null`.

## Decision

1. A new feature-specific privileged helper `crates/taskmanager-msr-helper`
   under the ADR-023 framework: one fixed operation — enumerate existing
   `/dev/cpu/N/msr` nodes (sorted by `N`, capped at 1024), `pread` the five
   Intel registers above plus, for CPUID family 0x17–0x19 CPUs, the AMD
   P-state block of [ADR-049](049-amd-msr-readouts-safe-file-io.md), and
   read the CPUID identity leaves (0/1/0x16) once from the first node's
   read-only `cpuid` file; decode through a pure `decode_*` layer, emit ONE
   JSON object to stdout, exit. `#![forbid(unsafe_code)]`; no workspace
   dependencies; no flags; no writes.
2. **Vendor gate = honest per-register failure plus per-field plausibility
   ranges.** A register read that returns no data (`EIO` from the driver, or
   end-of-file in fixtures) means "not implemented on this CPU" and becomes
   `null` for that field; a value outside its documented physical range stays
   `null`. The Intel vendor gate keeps this shape with no CPUID dependency;
   CPUID enters only as the AMD family gate (ADR-049) and the BCLK
   enumeration (leaf 0x16 ECX) — both through the read-only `cpuid` node of
   the same `/dev/cpu` tree. Unknown families produce rows of honest nulls
   rather than guessed numbers.
3. One fixed JSON contract, schema 1: SUCCESS is keyed off `packages`, ERROR
   kinds `permission_denied|no_msr|open_failed|read_failed` with exit codes
   2/3/4/5. `/dev/cpu` present but empty is an honest SUCCESS with an empty
   `packages` list; a missing `/dev/cpu` is `no_msr`.
4. `EscalationFeature::CpuMsr` appends one row to the Boundary-3 column (`ALL`
   grows to 8): the crossing is `polkit::invoke_msr_helper` driving
   `/usr/libexec/taskforest-msr-helper` through `pkexec`, authorized by the
   `io.github.YellowWhiteBlackCat.TaskForest.msr-helper` action with
   `auth_admin_keep`, probed by the `PolkitGate` filesystem twin (installed
   action + helper on Linux; typed `Unsupported` elsewhere), and surfaced by
   the CLI `--msr` flag. No generalization of the framework — one variant, one
   helper, one action, the same shape as the SMBIOS/RAPL rows before it.

## Consequences

- The four audited `unsafe` crates remain the only unsafe trust roots; the MSR
  crossing is auditable as a small amount of std-only safe Rust.
- The Boundary-3 table, system-install manifest, PKGBUILD/rpm `%files`,
  release smoke, install-manager script, polkit README and the escalation
  framework gate all carry the new row in this same change (hard cutover).
- `bclk_mhz` is populated only from the CPUID 0x16 ECX enumeration; every
  MSR-register and TSC-timing avenue was researched and rejected (the
  amendment above records the math and sources). When the leaf is not
  enumerated the field is `null`, and consumers must render it as
  unavailable rather than deriving a frequency from the multipliers.
- Code-chain closure does not replace the on-box pkexec receipt; the live
  authorization, denial and uninstall receipts are separate acceptance items,
  as for the other helpers.

## Verification

Fixture-backed headless tests cover the pure decode tables (documented bit
layouts, boundary values, garbage rejection), the fs walk over fixture
`/dev/cpu` trees (sorting, cap, empty, missing, denial classification, the
CPUID identity gates, the AMD register-set switch and sparse P-state-block
fixture), the JSON envelopes, the escalation parser/process semantics, the
gate probe twins, the polkit policy template, and the CLI rendering.
Commands:
`cargo nextest run -p taskmanager-msr-helper -j 4 --all-targets`,
`cargo nextest run -p taskmanager-escalation -j 4 --all-targets`,
`cargo nextest run -p taskmanager -j 4 --all-targets --test logic`
(see `crates/taskmanager-msr-helper/README.md`).
