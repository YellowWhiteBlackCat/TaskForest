//! The MSR sweep — pure safe file reads, parameterized by the `/dev/cpu` root
//! so tests run against fixture trees instead of the live (0600 root-only)
//! nodes.
//!
//! On Linux an MSR read is plain file I/O (ADR-048): open `/dev/cpu/N/msr`
//! read-only and `pread` 8 little-endian bytes AT THE REGISTER-ADDRESS
//! OFFSET. No `ioctl` and no syscall wrapper exist on this path — the same
//! mechanism libcpuid (CPU-X's MSR engine) uses — so this helper adds no new
//! `unsafe` trust root. The read-only `/dev/cpu/N/cpuid` sibling node is the
//! same kind of plain file I/O (one 16-byte `pread` per leaf at the
//! leaf-number offset) and carries no write path at all
//! (`arch/x86/kernel/cpuid.c` registers no `.write`).
//!
//! The sweep enumerates the numeric node directories (the kernel layout is
//! `/dev/cpu/<N>/msr` with a bare number name), sorts by `N`, caps at
//! [`MAX_CPU_NODES`], reads the CPUID identity leaves from the FIRST node's
//! `cpuid` file, then reads per node the register set selected by the CPUID
//! family gate: the five Intel registers below, or the AMD P-state block of
//! ADR-049 (family 0x17–0x19), and decodes them through the pure functions
//! of this module.
//!
//! Verified Intel register semantics (Intel SDM, kernel
//! `intel_tcc.c`/`coretemp.c`, libcpuid `rdtsc.c`; ADR-048 records the
//! sources):
//! * `MSR_PLATFORM_INFO` 0xCE bits 47:40 — Maximum Efficiency Ratio (min
//!   multiplier);
//! * `MSR_TURBO_RATIO_LIMIT` 0x1AD bits 7:0 — maximum 1-core turbo ratio;
//! * `IA32_PERF_STATUS` 0x198 bits 15:0 — current performance ratio, and
//!   bits 47:32 ÷ 2^13 volts — P-state core voltage (0 = not populated);
//! * `MSR_TEMPERATURE_TARGET` 0x1A2 bits 23:16 — TjMax (0 = not populated);
//! * `IA32_PACKAGE_THERM_STATUS` 0x1B1 bits 23:16 — package digital readout,
//!   valid only when bit 31 is set; temperature = TjMax − readout.
//!
//! Verified AMD semantics (AMD PPR family 17h, BKDG family 15h p.50,
//! libcpuid `rdtsc.c` = CPU-X's engine; ADR-049 records the sources and the
//! structurally unreachable SMN paths):
//! * `MSR_PSTATE_S` 0xC0010063 bits 2:0 — CurPstate;
//! * `MSR_PSTATE_0..7` 0xC0010064..0xC001006B — PstateEn bit 63, CpuFid
//!   bits 7:0, CpuDfsId bits 13:8, CpuVid bits 21:14;
//! * multiplier = (CpuFid ÷ CpuDfsId) × 2 ("CoreCOF is (CpuFid/CpuDfsId)*200"
//!   in the PPR); Vcore = 1.550 − 0.00625 × CpuVid (SVI2).
//!
//! Verified base-clock derivation (ADR-048 amendment): CPUID leaf 0x16 ECX
//! bits 15:0 is the SDM-defined "Bus (Reference) Frequency" in MHz — the
//! same leaf the kernel consumes in `native_calibrate_tsc`. Every
//! MSR-register avenue was rejected (ADR-048 records why); this enumeration
//! is the only verified source.
//!
//! Honesty rules:
//! * `/dev/cpu` missing → `no_msr`; present but empty → SUCCESS with an empty
//!   `packages` list (never a fabricated error);
//! * a register read returning no data (`EIO` from the driver — the register
//!   is not implemented on this CPU — or end-of-file in fixtures) makes that
//!   FIELD `null`; the node still reports its other readouts;
//! * a decoded value outside its documented plausibility range stays `null` —
//!   the vendor gate is per-register honesty, so unknown or out-of-window
//!   CPUs yield rows of nulls instead of guessed numbers;
//! * `bclk_mhz` is `null` unless CPUID leaf 0x16 enumerates a bus frequency
//!   inside the plausibility envelope;
//! * AMD temperature and BCLK stay `null`: no MSR-indexed path exists
//!   (ADR-049) — temperature already reaches the product unprivileged through
//!   the k10temp/zenpower hwmon chips;
//! * ANY other I/O failure is a typed ERROR for the whole sweep — a partial
//!   node list would understate the machine.

use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;
use std::path::Path;

use crate::json::{ErrorKindJson, PackageReadingJson};

/// `MSR_PLATFORM_INFO` — max non-turbo ratio (bits 15:8), max efficiency
/// ratio (bits 47:40).
const MSR_PLATFORM_INFO: u64 = 0xCE;
/// `MSR_TURBO_RATIO_LIMIT` — 1-core max ratio in bits 7:0.
const MSR_TURBO_RATIO_LIMIT: u64 = 0x1AD;
/// `IA32_PERF_STATUS` — current ratio in bits 15:0; P-state core voltage in
/// bits 47:32 (units of 1/8192 V).
const MSR_IA32_PERF_STATUS: u64 = 0x198;
/// `MSR_TEMPERATURE_TARGET` — TjMax in bits 23:16.
const MSR_TEMPERATURE_TARGET: u64 = 0x1A2;
/// `IA32_PACKAGE_THERM_STATUS` — package digital readout in bits 23:16,
/// readout-valid bit 31.
const MSR_IA32_PACKAGE_THERM_STATUS: u64 = 0x1B1;

/// `MSR_PSTATE_S` — CurPstate in bits 2:0 (AMD PPR family 17h).
const MSR_PSTATE_STATUS: u64 = 0xC0010063;
/// `MSR_PSTATE_0` — first of the eight P-state registers 0xC0010064..
/// 0xC001006B (PstateEn bit 63, CpuFid bits 7:0, CpuDfsId bits 13:8, CpuVid
/// bits 21:14). P-state 0 is Pb0, the highest-performance boosted state
/// (libcpuid `rdtsc.c`).
const MSR_PSTATE_0: u64 = 0xC0010064;
/// Number of P-state registers in the AMD block (ADR-049).
const PSTATE_REGISTERS: usize = 8;

/// CPUID leaf 0 — max standard leaf in EAX (the leaf-0x16 support gate).
const CPUID_LEAF_MAX: u64 = 0x0;
/// CPUID leaf 1 — family/model/stepping in EAX (the AMD decode gate).
const CPUID_LEAF_VERSION: u64 = 0x1;
/// CPUID leaf 0x16 — "Processor Frequency Information": ECX bits 15:0 carry
/// the Bus (Reference) Frequency in MHz (Intel SDM; consumed by the kernel's
/// `native_calibrate_tsc` — ADR-048 amendment).
const CPUID_LEAF_FREQUENCY: u64 = 0x16;
/// Each CPUID leaf is one 16-byte read (EAX, EBX, ECX, EDX).
const CPUID_LEAF_BYTES: usize = 16;

/// The `errno` value the Linux msr driver reports when the CPU faults the
/// register access (`rdmsr` #GP → `-EIO`): the register is not implemented on
/// this CPU. Hardcoded because the helper is std-only (no libc edge).
const EIO: i32 = 5;

/// Every MSR register is one 8-byte little-endian word at its address offset.
const MSR_WORD_BYTES: usize = 8;

/// Enumeration ceiling: nodes with `N >= 1024` are ignored, bounding the
/// sweep against a hostile or broken `/dev/cpu` layout.
const MAX_CPU_NODES: usize = 1024;

/// Plausibility ranges for the decoded fields — the documented physical
/// envelope of the fleet, used to reject garbage (e.g. non-Intel values at
/// these addresses, or a leaf-0 alias carrying vendor ASCII in ECX) rather
/// than to clamp or fabricate.
const TJMAX_MIN_C: f32 = 40.0;
const TJMAX_MAX_C: f32 = 130.0;
const TEMPERATURE_MIN_C: f32 = -40.0;
const TEMPERATURE_MAX_C: f32 = 150.0;
const MULTIPLIER_MIN: f32 = 2.0;
const MULTIPLIER_MAX: f32 = 128.0;
const RATIO_MAX: f32 = 128.0;
const VCORE_MIN_V: f32 = 0.1;
const VCORE_MAX_V: f32 = 2.0;
const BCLK_MIN_MHZ: f32 = 20.0;
const BCLK_MAX_MHZ: f32 = 500.0;

/// AMD family window with a verified P-state register layout (libcpuid
/// `rdtsc.c` gates its Zen decodes to exactly these): 0x17 = Zen/Zen+/Zen2,
/// 0x18 = Hygon Dhyana, 0x19 = Zen3/Zen4. Zen 5 (0x1A) is excluded upstream
/// (CPU-X #411: wrong values on Zen 5) and pre-0x17 parts use different
/// field layouts — both decode to honest nulls.
const AMD_FAMILY_MIN: u32 = 0x17;
const AMD_FAMILY_MAX: u32 = 0x19;
/// SVI2 VID decode (AMD BKDG family 15h p.50, carried forward for family
/// 17h+): V = 1.550 − 0.00625 × CpuVid.
const AMD_SVI2_BASE_V: f32 = 1.550;
const AMD_SVI2_VID_STEP_V: f32 = 0.00625;

/// The sweep's terminal result: per-node readouts sorted by CPU index, or a
/// typed error.
pub enum ReadOutcome {
    Packages { packages: Vec<PackageReadingJson> },
    Error(ReadError),
}

/// A typed sweep failure, already carrying the contract error kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub kind: ErrorKindJson,
    pub detail: String,
}

/// The five raw Intel register words of one node. `None` = the register
/// returned no data (not implemented on this CPU) — an honest absence, not a
/// zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RawRegisters {
    pub platform_info: Option<u64>,
    pub turbo_ratio_limit: Option<u64>,
    pub perf_status: Option<u64>,
    pub temperature_target: Option<u64>,
    pub package_therm_status: Option<u64>,
    /// `MSR_PSTATE_S` (0xC0010063) — the AMD current-P-state selector.
    pub pstate_status: Option<u64>,
    /// The AMD P-state block 0xC0010064..0xC001006B, lowest index first
    /// (P-state 0 = Pb0). Only read when the CPUID family gate identifies a
    /// family 0x17–0x19 CPU.
    pub pstates: [Option<u64>; PSTATE_REGISTERS],
}

/// Package-wide CPU facts read once from the first node's read-only
/// `/dev/cpu/N/cpuid` file: the CPUID display family (the register-set gate)
/// and the base-clock enumeration (CPUID 0x16 ECX). All fields are `None`
/// when the node carries no `cpuid` file — the sweep then keeps the Intel
/// register set and every gated field stays honestly `null`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CpuIdentity {
    /// CPUID leaf 1 display family (`None` = leaf unreadable).
    pub family: Option<u32>,
    /// Base clock in MHz from CPUID 0x16 ECX bits 15:0 (`None` = not
    /// enumerated or outside the plausibility envelope).
    pub bclk_mhz: Option<f32>,
}

/// Whether `family` sits inside the verified AMD decode window.
fn is_amd_zen(family: Option<u32>) -> bool {
    family.is_some_and(|family| (AMD_FAMILY_MIN..=AMD_FAMILY_MAX).contains(&family))
}

/// The MSR address of AMD P-state `index` (0..=7): the block spans the
/// CONSECUTIVE addresses 0xC0010064..0xC001006B — each pread of 8 bytes at
/// the address reads that one register on the real device.
pub fn amd_pstate_address(index: usize) -> u64 {
    MSR_PSTATE_0 + index as u64
}

/// Collect the MSR readouts of every existing `/dev/cpu/N/msr` node under
/// `dev_cpu_root`. See the module docs for the honesty rules.
pub fn collect_msr_readings(dev_cpu_root: &Path) -> ReadOutcome {
    let entries = match std::fs::read_dir(dev_cpu_root) {
        Ok(entries) => entries,
        Err(error) => {
            return ReadOutcome::Error(classify_root_error(&error, dev_cpu_root));
        }
    };
    let mut nodes: Vec<(u32, std::path::PathBuf)> = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            return ReadOutcome::Error(ReadError {
                kind: ErrorKindJson::OpenFailed,
                detail: format!("iterating {} failed", dev_cpu_root.display()),
            });
        };
        // Only numeric node directories (`/dev/cpu/<N>`, bare number names)
        // are MSR nodes; anything else under /dev/cpu (`cpuid` siblings of
        // the msr files live INSIDE the node dirs, not here) is ignored.
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if let Ok(index) = name.parse::<u32>() {
            nodes.push((index, entry.path()));
        }
    }
    nodes.sort_by_key(|(index, _)| *index);
    nodes.retain(|(index, _)| (*index as usize) < MAX_CPU_NODES);
    // The CPUID identity (family gate + BCLK enumeration) is package-wide:
    // one read from the first node's read-only cpuid file.
    let identity = match nodes.first() {
        Some((_, dir)) => match read_identity(dir) {
            Ok(identity) => identity,
            Err(error) => return ReadOutcome::Error(error),
        },
        None => CpuIdentity::default(),
    };
    let amd = is_amd_zen(identity.family);
    let mut packages = Vec::new();
    for (cpu, dir) in &nodes {
        // A numeric node directory whose msr file is absent is not an
        // existing node:
        // it contributes no row at all.
        if let Some(raw) = match read_node(&dir.join("msr"), amd) {
            Ok(raw) => raw,
            Err(error) => {
                return ReadOutcome::Error(ReadError {
                    kind: error.kind,
                    detail: format!("cpu {cpu}: {}", error.detail),
                });
            }
        } {
            packages.push(decode_reading(*cpu, &raw, &identity));
        }
    }
    ReadOutcome::Packages { packages }
}

/// Open one node's msr file and read its register set: the five Intel
/// registers, or — when `amd` (the CPUID family gate said family 0x17–0x19) —
/// the AMD P-state block of ADR-049. `Ok(None)` = the numeric node directory
/// exists but its msr file does not (offline CPU, or the msr
/// driver not loaded): an honest skip, not a row of nulls and not an error;
/// a node present but unreadable is a typed error.
fn read_node(msr_path: &Path, amd: bool) -> Result<Option<RawRegisters>, ReadError> {
    let file = match File::open(msr_path) {
        Ok(file) => file,
        Err(error) => {
            return match classify_open_error(&error, msr_path) {
                None => Ok(None),
                Some(error) => Err(error),
            };
        }
    };
    let read = |address: u64| read_msr(&file, address, msr_path);
    if amd {
        let mut pstates = [None; PSTATE_REGISTERS];
        for (index, slot) in pstates.iter_mut().enumerate() {
            *slot = read(amd_pstate_address(index))?;
        }
        return Ok(Some(RawRegisters {
            pstate_status: read(MSR_PSTATE_STATUS)?,
            pstates,
            ..RawRegisters::default()
        }));
    }
    Ok(Some(RawRegisters {
        platform_info: read(MSR_PLATFORM_INFO)?,
        turbo_ratio_limit: read(MSR_TURBO_RATIO_LIMIT)?,
        perf_status: read(MSR_IA32_PERF_STATUS)?,
        temperature_target: read(MSR_TEMPERATURE_TARGET)?,
        package_therm_status: read(MSR_IA32_PACKAGE_THERM_STATUS)?,
        ..RawRegisters::default()
    }))
}

/// Read the CPUID identity leaves from a node's read-only `cpuid` file (one
/// 16-byte `pread` per leaf, offset = leaf number). A missing file, a
/// permission denial on it or any non-absence failure follows the sweep's
/// error rules; an unimplemented leaf (no data) is an honest absence.
fn read_identity(node_dir: &Path) -> Result<CpuIdentity, ReadError> {
    let path = node_dir.join("cpuid");
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) => {
            return match classify_open_error(&error, &path) {
                None => Ok(CpuIdentity::default()),
                Some(error) => Err(error),
            };
        }
    };
    let leaf = |index: u64| read_cpuid_leaf(&file, index, &path);
    let max_leaf = leaf(CPUID_LEAF_MAX)?.map(|registers| registers.0);
    let family = leaf(CPUID_LEAF_VERSION)?.map(|registers| decode_family(registers.0));
    let frequency = leaf(CPUID_LEAF_FREQUENCY)?.map(|registers| registers.2);
    Ok(CpuIdentity {
        family,
        bclk_mhz: decode_bclk_mhz(max_leaf, frequency),
    })
}

/// Read one 16-byte CPUID leaf (EAX, EBX, ECX, EDX as little-endian u32s) at
/// the leaf-number offset. `Ok(None)` = the leaf returned no data; any other
/// failure is a typed `read_failed`.
fn read_cpuid_leaf(
    file: &File,
    leaf: u64,
    path: &Path,
) -> Result<Option<(u32, u32, u32, u32)>, ReadError> {
    let mut buffer = [0u8; CPUID_LEAF_BYTES];
    match file.read_exact_at(&mut buffer, leaf) {
        Ok(()) => Ok(Some(le_u32_at(&buffer, 0))),
        Err(error) => map_register_error(&error).map_err(|kind| ReadError {
            kind,
            detail: format!("read cpuid leaf 0x{leaf:X} at {}: {error}", path.display()),
        }),
    }
}

/// Decode four little-endian u32 words out of a 16-byte register image,
/// without panicking slice conversions.
fn le_u32_at(buffer: &[u8; CPUID_LEAF_BYTES], offset: usize) -> (u32, u32, u32, u32) {
    let word = |index: usize| {
        u32::from(buffer[offset + index * 4])
            | u32::from(buffer[offset + index * 4 + 1]) << 8
            | u32::from(buffer[offset + index * 4 + 2]) << 16
            | u32::from(buffer[offset + index * 4 + 3]) << 24
    };
    (word(0), word(1), word(2), word(3))
}

/// Read one 8-byte MSR word at the register-address offset. `Ok(None)` = the
/// register returned no data (driver `EIO`, or end-of-file in fixtures): the
/// register is not implemented on this CPU. Any other failure is a typed
/// `read_failed`.
fn read_msr(file: &File, address: u64, msr_path: &Path) -> Result<Option<u64>, ReadError> {
    let mut buffer = [0u8; MSR_WORD_BYTES];
    match file.read_exact_at(&mut buffer, address) {
        Ok(()) => Ok(Some(u64::from_le_bytes(buffer))),
        Err(error) => map_register_error(&error).map_err(|kind| ReadError {
            kind,
            detail: format!("read msr 0x{address:X} at {}: {error}", msr_path.display()),
        }),
    }
}

/// Classify a per-register read failure: no data (`EIO` from the driver or
/// end-of-file) → the register is absent (`Ok(None)`); anything else →
/// `read_failed`. Generic over the register word type (MSR u64 words, CPUID
/// leaf register tuples).
fn map_register_error<T>(error: &io::Error) -> Result<Option<T>, ErrorKindJson> {
    if error.raw_os_error() == Some(EIO) || error.kind() == io::ErrorKind::UnexpectedEof {
        Ok(None)
    } else {
        Err(ErrorKindJson::ReadFailed)
    }
}

/// Decode the raw register words of one node into the contract row, using
/// the register set the CPUID family gate selected: the Intel decodes, or the
/// AMD P-state decodes of ADR-049 for family 0x17–0x19. Fields whose
/// register is absent or whose value fails its plausibility range stay
/// `null` — never a fabricated zero. `bclk_mhz` comes from the package-wide
/// CPUID 0x16 enumeration.
pub fn decode_reading(cpu: u32, raw: &RawRegisters, identity: &CpuIdentity) -> PackageReadingJson {
    if is_amd_zen(identity.family) {
        PackageReadingJson {
            cpu,
            bclk_mhz: identity.bclk_mhz,
            // No MSR-indexed temperature path exists on AMD (ADR-049); the
            // product already reads k10temp/zenpower hwmon unprivileged.
            temperature_c: None,
            multiplier: decode_amd_multiplier_current(raw.pstate_status, &raw.pstates),
            multiplier_min: decode_amd_multiplier_min(&raw.pstates),
            multiplier_max: decode_amd_multiplier_max(&raw.pstates),
            vcore_v: decode_amd_vcore_v(raw.pstate_status, &raw.pstates),
        }
    } else {
        PackageReadingJson {
            cpu,
            bclk_mhz: identity.bclk_mhz,
            temperature_c: decode_temperature_c(raw.temperature_target, raw.package_therm_status),
            multiplier: decode_multiplier(raw.perf_status),
            multiplier_min: decode_multiplier_min(raw.platform_info),
            multiplier_max: decode_multiplier_max(raw.turbo_ratio_limit),
            vcore_v: decode_vcore_v(raw.perf_status),
        }
    }
}

/// CPUID display family from leaf 1 EAX: base family bits 11:8, extended
/// family bits 27:20 added only when the base family is 0xF (SDM CPUID
/// "Intel® 64 Architecture" EAX format).
pub fn decode_family(version_eax: u32) -> u32 {
    let base_family = (version_eax >> 8) & 0xf;
    if base_family == 0xf {
        base_family + ((version_eax >> 20) & 0xff)
    } else {
        base_family
    }
}

/// Base clock from CPUID leaf 0x16 ECX bits 15:0 — the SDM "Bus (Reference)
/// Frequency" in MHz. `None` when the leaf is unreadable, the CPU's max
/// standard leaf predates 0x16 (an out-of-range leaf aliases to leaf 0,
/// whose vendor-string bytes must never decode as a clock), the field is
/// unenumerated (0), or the value leaves the plausibility envelope.
pub fn decode_bclk_mhz(max_leaf: Option<u32>, frequency_ecx: Option<u32>) -> Option<f32> {
    let max_leaf = max_leaf?;
    if max_leaf < CPUID_LEAF_FREQUENCY as u32 {
        return None;
    }
    let mhz = (frequency_ecx? & 0xffff) as f32;
    (BCLK_MIN_MHZ..=BCLK_MAX_MHZ).contains(&mhz).then_some(mhz)
}

/// One AMD P-state register → multiplier: "CoreCOF is (CpuFid/CpuDfsId)*200"
/// (AMD PPR family 17h), so the multiplier over the 100 MHz base clock is
/// `fid/did × 2` — the exact decode of libcpuid `rdtsc.c`. `None` when the
/// register is unimplemented, PstateEn (bit 63) is clear (the PPR marks the
/// rest of the register invalid until enabled), CpuDfsId is 0, or the value
/// leaves the plausibility envelope.
pub fn decode_amd_multiplier(pstate: Option<u64>) -> Option<f32> {
    let register = pstate?;
    if register >> 63 == 0 {
        return None;
    }
    let dfs_id = (register >> 8) & 0x3f;
    let fid = register & 0xff;
    if dfs_id == 0 {
        return None;
    }
    let multiplier = (fid as f32 / dfs_id as f32) * 2.0;
    (MULTIPLIER_MIN..=MULTIPLIER_MAX)
        .contains(&multiplier)
        .then_some(multiplier)
}

/// The current-P-state index selected by `MSR_PSTATE_S` bits 2:0.
fn amd_current_pstate(status: Option<u64>) -> Option<usize> {
    Some((status? & 0x7) as usize)
}

/// Current multiplier: the P-state register selected by CurPstate.
pub fn decode_amd_multiplier_current(
    status: Option<u64>,
    pstates: &[Option<u64>; PSTATE_REGISTERS],
) -> Option<f32> {
    decode_amd_multiplier(pstates[amd_current_pstate(status)?])
}

/// Minimum multiplier: the LAST (lowest-performance) P-state register with
/// PstateEn set — libcpuid scans down from MSR_PSTATE_7 for the first
/// enabled register and decodes that one.
pub fn decode_amd_multiplier_min(pstates: &[Option<u64>; PSTATE_REGISTERS]) -> Option<f32> {
    let last_enabled = pstates
        .iter()
        .rev()
        .copied()
        .find(|register| register.is_some_and(|value| value >> 63 != 0))?;
    decode_amd_multiplier(last_enabled)
}

/// Maximum multiplier: P-state 0 is Pb0, the highest-performance boosted
/// state (libcpuid: "MSRC001_0064 is Pb0").
pub fn decode_amd_multiplier_max(pstates: &[Option<u64>; PSTATE_REGISTERS]) -> Option<f32> {
    decode_amd_multiplier(pstates[0])
}

/// Vcore from the CURRENT P-state's CpuVid (bits 21:14) through the SVI2
/// decode `V = 1.550 − 0.00625 × CpuVid` (AMD BKDG family 15h p.50, valid
/// for family 17h+; libcpuid identical). Requires PstateEn like every AMD
/// P-state decode; out-of-envelope results stay `null`.
pub fn decode_amd_vcore_v(
    status: Option<u64>,
    pstates: &[Option<u64>; PSTATE_REGISTERS],
) -> Option<f32> {
    let register = pstates[amd_current_pstate(status)?]?;
    if register >> 63 == 0 {
        return None;
    }
    let vid = (register >> 14) & 0xff;
    let volts = AMD_SVI2_BASE_V - AMD_SVI2_VID_STEP_V * vid as f32;
    (VCORE_MIN_V..=VCORE_MAX_V)
        .contains(&volts)
        .then_some(volts)
}

/// Package temperature = TjMax (`0x1A2` bits 23:16) − package digital readout
/// (`0x1B1` bits 23:16, valid only when bit 31 is set). `None` when either
/// register is absent, TjMax is unpopulated or implausible, the readout is
/// flagged invalid, or the result leaves the physical envelope.
pub fn decode_temperature_c(temp_target: Option<u64>, pkg_status: Option<u64>) -> Option<f32> {
    let tjmax = ((temp_target? >> 16) & 0xff) as f32;
    if !(TJMAX_MIN_C..=TJMAX_MAX_C).contains(&tjmax) {
        return None;
    }
    let status = pkg_status?;
    // Bit 31 gates the digital readout (kernel intel_tcc: "temperature is
    // beyond the valid thermal sensor range").
    if status & (1 << 31) == 0 {
        return None;
    }
    let readout = ((status >> 16) & 0xff) as f32;
    let temperature = tjmax - readout;
    (TEMPERATURE_MIN_C..=TEMPERATURE_MAX_C)
        .contains(&temperature)
        .then_some(temperature)
}

/// Current performance ratio: `0x198` bits 15:0.
pub fn decode_multiplier(perf_status: Option<u64>) -> Option<f32> {
    let ratio = (perf_status? & 0xffff) as f32;
    (MULTIPLIER_MIN..=MULTIPLIER_MAX)
        .contains(&ratio)
        .then_some(ratio)
}

/// Maximum efficiency ratio (minimum multiplier): `0xCE` bits 47:40.
pub fn decode_multiplier_min(platform_info: Option<u64>) -> Option<f32> {
    let ratio = ((platform_info? >> 40) & 0xff) as f32;
    (1.0..=RATIO_MAX).contains(&ratio).then_some(ratio)
}

/// Maximum 1-core turbo ratio: `0x1AD` bits 7:0.
pub fn decode_multiplier_max(turbo_ratio_limit: Option<u64>) -> Option<f32> {
    let ratio = (turbo_ratio_limit? & 0xff) as f32;
    (1.0..=RATIO_MAX).contains(&ratio).then_some(ratio)
}

/// P-state core voltage: `0x198` bits 47:32 in units of 1/8192 V (Intel SDM:
/// `MSR_PERF_STATUS[37:32] * (float) 1/(2^13)`). A zero field is not a zero
/// volt reading — modern Intel leaves it unpopulated, which decodes to
/// `None`.
pub fn decode_vcore_v(perf_status: Option<u64>) -> Option<f32> {
    let raw = (perf_status? >> 32) & 0xffff;
    if raw == 0 {
        return None;
    }
    let volts = raw as f32 / 8192.0;
    (VCORE_MIN_V..=VCORE_MAX_V)
        .contains(&volts)
        .then_some(volts)
}

/// Classify a `/dev/cpu` root open failure: missing root → `no_msr`;
/// `EACCES`/`EPERM` → `permission_denied`; anything else → `open_failed`.
fn classify_root_error(error: &io::Error, root: &Path) -> ReadError {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => ErrorKindJson::NoMsr,
        io::ErrorKind::PermissionDenied => ErrorKindJson::PermissionDenied,
        _ => ErrorKindJson::OpenFailed,
    };
    ReadError {
        kind,
        detail: format!("open {}: {error}", root.display()),
    }
}

/// Classify a node-open failure. `None` = the node's msr file is gone
/// (`ENOENT`): an honest skip, not an error; `EACCES`/`EPERM` →
/// `permission_denied` (the escalatable denial this helper exists to cross);
/// anything else → `open_failed`.
fn classify_open_error(error: &io::Error, msr_path: &Path) -> Option<ReadError> {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => return None,
        io::ErrorKind::PermissionDenied => ErrorKindJson::PermissionDenied,
        _ => ErrorKindJson::OpenFailed,
    };
    Some(ReadError {
        kind,
        detail: format!("open {}: {error}", msr_path.display()),
    })
}

#[cfg(test)]
#[path = "../tests/headless/msr_helper_msr_read.rs"]
mod tests;
