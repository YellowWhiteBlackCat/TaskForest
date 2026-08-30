# ADR-049: AMD MSR readouts within the safe file-I/O crossing

Status: accepted

- Extends: [ADR-048 (MSR readout helper as a safe file-I/O crossing)](048-msr-read-helper.md).

## Context

ADR-048 shipped the MSR helper as an Intel-only sweep. The follow-up question:
which AMD readouts can a `#![forbid(unsafe_code)]` helper reach through PLAIN
FILE I/O only — no `mmap`, no port I/O, no shelling out, no writes? The
reference implementation is CPU-X (5.x delegates all MSR work to libcpuid's
`rdtsc.c`/`rdmsr.c` through its daemon; its own core falls back to the k10temp
kernel module for AMD temperature), cross-checked against the Linux kernel's
own consumers (`k10temp.c`, `amd_node.c`, `cpuid.c`, `amd-pstate`).

## Verified findings

1. **The AMD P-state registers are plain RDMSR territory.** libcpuid reads
   `MSR_PSTATE_S` 0xC0010063 (CurPstate, bits 2:0) and the consecutive block
   `MSR_PSTATE_0..7` 0xC0010064..0xC001006B (PstateEn bit 63, CpuFid bits
   7:0, CpuDfsId bits 13:8, CpuVid bits 21:14) through the same
   `/dev/cpu/N/msr` pread path as the Intel registers — no SMN, no PCI.
2. **Decodes** (AMD PPR family 17h; BKDG family 15h p.50; libcpuid
   `get_amd_multipliers`/`get_info_voltage`):
   - multiplier = (CpuFid ÷ CpuDfsId) × 2 — the PPR's "CoreCOF is
     (CpuFid/CpuDfsId)*200" expressed over the 100 MHz base clock;
   - current P-state = `MSR_PSTATE_S` CurPstate; maximum = P-state 0 (Pb0);
     minimum = the last PstateEn-set register scanning down from P-state 7;
   - Vcore = 1.550 − 0.00625 × CpuVid (SVI2).
3. **AMD temperature has NO MSR-indexed path.** libcpuid's temperature decode
   has only an Intel branch; Tctl/Tdie live in SMN (0x59800 and per-CCD
   neighbors), which the kernel reaches through `amd_smn_read()` — a PCI
   config index/data pair (0x60/0x64) on the Data Fabric F3 device, guarded
   by a kernel-only `smn_mutex` (`arch/x86/kernel/amd_node.c`). A userspace
   replica would need a **pwrite** to the address register: it violates the
   helper's no-writes red line AND races every kernel SMN consumer (an
   interleaved k10temp read would see the wrong SMN address selected).
4. **MMIO is out of reach by construction**: mapping a BAR needs `mmap`,
   which safe std does not expose; pre-Zen index/data ports need `iopl`.

## Decision

1. The helper's sweep becomes register-set-per-CPU: the five Intel registers
   of ADR-048, or — when the CPUID family gate identifies family 0x17–0x19
   (Zen/Zen+/Zen2, Hygon, Zen3/Zen4) — the AMD block `MSR_PSTATE_S` +
   `MSR_PSTATE_0..7`. The gate reads CPUID leaves 0/1/0x16 from the
   read-only `/dev/cpu/N/cpuid` node (one 16-byte pread per leaf; the driver
   registers no `.write`), the same `/dev/cpu` tree the helper already
   enumerates. No vendor string is needed: CPUID display families 0x17–0x19
   are unambiguously AMD/Hygon.
2. AMD rows fill `multiplier`, `multiplier_min`, `multiplier_max`, `vcore_v`
   through the verified decodes above. Stricter than libcpuid, every AMD
   P-state decode requires PstateEn (the PPR marks the rest of the register
   invalid without it) and rejects CpuDfsId 0 (a division by zero in the
   reference code); every value passes the existing plausibility envelopes.
3. **Typed absence**: `temperature_c` and `bclk_mhz` stay `null` on AMD —
   temperature already reaches the product unprivileged through the
   k10temp/zenpower hwmon chips that
   `taskmanager-platform-linux`'s CPU sources read, so the helper must not
   duplicate it; BCLK via TSC timing is ADR-048-rejected, and CPUID 0x16 is
   not enumerated on Zen. Families outside 0x17–0x19 — including Zen 5
   (0x1A, excluded upstream per CPU-X issue #411: wrong values on Zen 5) and
   the differently-laid-out pre-Zen families — decode nothing and produce
   rows of honest nulls.

## Consequences

- AMD users gain the multiplier readouts and an SVI2 Vcore through the same
  escalated one-shot JSON contract — schema and escalation lane unchanged
  (fields were already number-or-null).
- SMN-only telemetry (Tctl/Tdie detail, SVI2 power/current, per-CCD
  temperatures) is recorded as structurally unreachable from this helper in
  safe std: reaching it would require PCI-config writes or `mmap`, both
  outside the file-I/O trust argument of ADR-048.
- The CurPstate view is the hardware P-state ladder; on CPPC/EPP-managed
  parts it is the same value CPU-X displays, not a kernel-computed
  effective frequency.

## Verification

Fixture-backed headless tests: the PPR fid/dfs decode table (boundary
values, PstateEn and CpuDfsId-0 rejection, envelope garbage), the SVI2 VID
table, the current/Pb0/last-enabled selection scans, the CPUID family
window (0x17/0x18/0x19 in; 0x1A, 0xF, family 6 out), the register-set
switch end-to-end (Intel bytes under an AMD gate stay null), and a sparse
fixture reading the true P-state addresses. Command:
`cargo nextest run -p taskmanager-msr-helper -j 4 --all-targets`
(see `crates/taskmanager-msr-helper/README.md`).
