//! Tests for the MSR sweep: the verified decode tables (documented bit
//! layouts, boundary values, garbage rejection) and the fs walk over fixture
//! `/dev/cpu` trees. No test touches the live host's `/dev/cpu`.

use super::*;
use crate::test_support::repo_temp_dir;
use std::fs;
use std::path::{Path, PathBuf};

/// A unique fixture `/dev/cpu` root under the repository `.tmp/` scratch.
fn fixture_root(tag: &str) -> PathBuf {
    repo_temp_dir().join(format!("tm_msr_walk_{tag}"))
}

/// Write one `<root>/<N>/msr` node (the kernel's layout: `/dev/cpu/0/msr`)
/// whose register words sit at their address offsets as they do on the real
/// character device (little-endian, 8 bytes at the register address).
fn write_msr_node(root: &Path, cpu: u32, registers: &[(u64, u64)]) {
    let dir = root.join(format!("{cpu}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("msr"), encode_registers(registers)).unwrap();
}

/// Write one `<root>/<N>/msr` node for the AMD P-state block. The register
/// addresses live at offset ~2 GiB, so the fixture is a sparse file
/// (set_len + write_at). A plain byte image can only carry NON-overlapping
/// register windows: the P-state registers sit at consecutive MSR addresses
/// (0xC0010064..0xC001006B), so 8-byte words there would clobber each other
/// even though the real device reads each address independently. The one
/// faithful multi-word subset is `MSR_PSTATE_S` (0xC0010063..0xC001006A)
/// plus P-state 7 (0xC001006B..0xC0010072) — exactly what the walk tests
/// below use.
fn write_amd_msr_node(root: &Path, cpu: u32, status: u64, pstates: &[(usize, u64)]) {
    use std::os::unix::fs::FileExt;
    let dir = root.join(format!("{cpu}"));
    fs::create_dir_all(&dir).unwrap();
    let file = fs::File::create(dir.join("msr")).unwrap();
    file.set_len(0xC001_00A4).unwrap();
    file.write_all_at(&status.to_le_bytes(), 0xC001_0063)
        .unwrap();
    for (index, value) in pstates {
        let address = amd_pstate_address(*index);
        file.write_all_at(&value.to_le_bytes(), address).unwrap();
    }
}

/// Write one `<root>/<N>/cpuid` node: each leaf is 16 bytes (EAX, EBX, ECX,
/// EDX little-endian) at the leaf-number offset, as the cpuid character
/// device lays them out. A plain file can only represent non-overlapping
/// leaf sets (leaf N spans bytes N..N+16); each fixture below picks a
/// representable subset — the real device synthesizes every leaf
/// independently.
fn write_cpuid_node(root: &Path, cpu: u32, leaves: &[(u64, [u32; 4])]) {
    let dir = root.join(format!("{cpu}"));
    fs::create_dir_all(&dir).unwrap();
    let end = leaves
        .iter()
        .map(|(leaf, _)| (*leaf as usize) + 16)
        .max()
        .unwrap_or(0);
    let mut bytes = vec![0u8; end];
    for (leaf, registers) in leaves {
        for (index, register) in registers.iter().enumerate() {
            let start = *leaf as usize + index * 4;
            bytes[start..start + 4].copy_from_slice(&register.to_le_bytes());
        }
    }
    fs::write(dir.join("cpuid"), bytes).unwrap();
}

/// Encode register words into a sparse byte image: offset = MSR address.
fn encode_registers(registers: &[(u64, u64)]) -> Vec<u8> {
    let end = registers
        .iter()
        .map(|(address, _)| (*address + 8) as usize)
        .max()
        .unwrap_or(0);
    let mut bytes = vec![0u8; end];
    for (address, value) in registers {
        bytes[*address as usize..*address as usize + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// A register set derived from the documented layouts (NOT a host capture):
/// TjMax 100 °C, package readout 42 (valid), current ratio 45, minimum ratio
/// 8, maximum turbo ratio 55, P-state voltage field 9984 (= 1.21875 V).
fn documented_register_set() -> Vec<(u64, u64)> {
    vec![
        (0xCE, 8 << 40),
        (0x1AD, 55),
        (0x198, (9984u64 << 32) | 45),
        (0x1A2, 100 << 16),
        (0x1B1, (1 << 31) | (42 << 16)),
    ]
}

// --- pure decode tables ----------------------------------------------------

#[test]
fn decode_temperature_follows_the_documented_tjmax_minus_readout_layout() {
    let temp_target = Some(100u64 << 16);
    let pkg_status = Some((1 << 31) | (42 << 16));
    assert_eq!(decode_temperature_c(temp_target, pkg_status), Some(58.0));
    // The readout is 8 bits wide; TjMax offsets of 0..255 stay in envelope.
    assert_eq!(
        decode_temperature_c(temp_target, Some((1 << 31) | (100 << 16))),
        Some(0.0),
    );
}

#[test]
fn decode_temperature_rejects_unpopulated_or_out_of_envelope_inputs() {
    let temp_target = Some(100u64 << 16);
    // Valid bit (0x1B1 bit 31) clear: the readout is not trustworthy.
    assert_eq!(decode_temperature_c(temp_target, Some(42 << 16)), None);
    // TjMax field zero: the CPU does not report it (kernel ENODATA case).
    assert_eq!(
        decode_temperature_c(Some(0), Some(1 << 31)),
        None,
        "unpopulated TjMax must stay null, never a fabricated 0-255 °C reading",
    );
    // Implausible TjMax (200 °C) — garbage rejection, not clamping.
    assert_eq!(
        decode_temperature_c(Some(200 << 16), Some(1 << 31)),
        None,
        "an implausible TjMax must not yield a temperature",
    );
    // A readout pushing the result below the physical envelope.
    assert_eq!(
        decode_temperature_c(temp_target, Some((1 << 31) | (255 << 16))),
        None,
    );
    // Either register absent.
    assert_eq!(decode_temperature_c(None, Some(1 << 31)), None);
    assert_eq!(decode_temperature_c(temp_target, None), None);
}

#[test]
fn decode_multipliers_read_the_documented_bit_fields() {
    // Current ratio lives in 0x198 bits 15:0 — even when the voltage field
    // (bits 47:32) is populated.
    assert_eq!(decode_multiplier(Some((9984u64 << 32) | 45)), Some(45.0));
    assert_eq!(
        decode_multiplier(Some(0)),
        None,
        "a zero ratio is not a P-state"
    );
    assert_eq!(decode_multiplier(Some(1000)), None, "1000x is not physical");
    assert_eq!(decode_multiplier(None), None);
    // Minimum ratio: 0xCE bits 47:40.
    assert_eq!(decode_multiplier_min(Some(8 << 40)), Some(8.0));
    assert_eq!(decode_multiplier_min(Some(0)), None);
    assert_eq!(decode_multiplier_min(None), None);
    // Maximum 1-core turbo ratio: 0x1AD bits 7:0; higher groups must not
    // leak into the decoded ratio.
    assert_eq!(decode_multiplier_max(Some((7u64 << 56) | 55)), Some(55.0));
    assert_eq!(decode_multiplier_max(Some(0)), None);
    assert_eq!(decode_multiplier_max(None), None);
}

#[test]
fn decode_vcore_uses_the_sdm_one_over_8192_scaling_and_rejects_unpopulated() {
    // 9984 / 8192 = 1.21875 V exactly (no float noise).
    assert_eq!(decode_vcore_v(Some(9984 << 32)), Some(1.21875));
    // A zero field is "not populated" (all modern Intel), never 0 V.
    assert_eq!(decode_vcore_v(Some(45)), None);
    // Out-of-envelope voltages (raw 0xFFFF ≈ 8 V) are garbage, not clamped.
    assert_eq!(decode_vcore_v(Some(0xFFFF << 32)), None);
    assert_eq!(decode_vcore_v(None), None);
}

#[test]
fn decode_reading_assembles_the_intel_contract_row() {
    let raw = RawRegisters {
        platform_info: Some(8 << 40),
        turbo_ratio_limit: Some(55),
        perf_status: Some((9984u64 << 32) | 45),
        temperature_target: Some(100 << 16),
        package_therm_status: Some((1 << 31) | (42 << 16)),
        ..RawRegisters::default()
    };
    let reading = decode_reading(3, &raw, &CpuIdentity::default());
    assert_eq!(
        reading,
        PackageReadingJson {
            cpu: 3,
            bclk_mhz: None,
            temperature_c: Some(58.0),
            multiplier: Some(45.0),
            multiplier_min: Some(8.0),
            multiplier_max: Some(55.0),
            vcore_v: Some(1.21875),
        },
        "without a cpuid node the Intel row keeps today's shape and bclk stays null",
    );
    // The CPUID 0x16 enumeration, when present, fills bclk for the row.
    let identified = decode_reading(
        3,
        &raw,
        &CpuIdentity {
            family: Some(6),
            bclk_mhz: Some(100.0),
        },
    );
    assert_eq!(identified.bclk_mhz, Some(100.0));
    assert_eq!(identified.multiplier, Some(45.0));
    // An all-absent register set decodes to honest nulls, not zeros.
    let empty = decode_reading(0, &RawRegisters::default(), &CpuIdentity::default());
    assert_eq!(empty.temperature_c, None);
    assert_eq!(empty.multiplier, None);
    assert_eq!(empty.multiplier_min, None);
    assert_eq!(empty.multiplier_max, None);
    assert_eq!(empty.vcore_v, None);
    assert_eq!(empty.bclk_mhz, None);
}

// --- CPUID identity gates ---------------------------------------------------

#[test]
fn decode_family_reads_the_extended_family_of_the_version_leaf() {
    // Intel family 6 (model 204): no extension added.
    assert_eq!(decode_family(0x000C_06D0), 6);
    // Zen (0x17): base 0xF + extended 0x8.
    assert_eq!(decode_family(0x0080_0F10), 0x17);
    // Hygon (0x18): base 0xF + extended 0x9.
    assert_eq!(decode_family(0x0090_0F10), 0x18);
    // Zen 3/4 (0x19): base 0xF + extended 0xA.
    assert_eq!(decode_family(0x00A0_0F10), 0x19);
    // Zen 5 (0x1A) and pre-Zen 15h decode outside the AMD window.
    assert_eq!(decode_family(0x00B0_0F10), 0x1A);
    assert_eq!(decode_family(0x0000_0F10), 0xF);
}

#[test]
fn decode_bclk_reads_the_sdm_bus_reference_frequency_with_both_gates() {
    // Enumerated leaf 0x16 ECX = 100 MHz -> the BCLK.
    assert_eq!(decode_bclk_mhz(Some(0x20), Some(100)), Some(100.0));
    assert_eq!(decode_bclk_mhz(Some(0x16), Some(133)), Some(133.0));
    // Unenumerated (0) or out-of-envelope values stay null — a leaf-0 alias
    // carrying vendor ASCII in ECX (>= 0x2020 MHz) is rejected here too.
    assert_eq!(decode_bclk_mhz(Some(0x20), Some(0)), None);
    assert_eq!(decode_bclk_mhz(Some(0x20), Some(19)), None);
    assert_eq!(decode_bclk_mhz(Some(0x20), Some(501)), None);
    assert_eq!(decode_bclk_mhz(Some(0x20), Some(0x6E69)), None);
    // The max-standard-leaf gate: a CPU whose CPUID ends before leaf 0x16
    // aliases the leaf to 0 (vendor bytes), which must never decode.
    assert_eq!(decode_bclk_mhz(Some(0x10), Some(100)), None);
    // Either leaf unreadable (no cpuid node / leaf not implemented).
    assert_eq!(decode_bclk_mhz(None, Some(100)), None);
    assert_eq!(decode_bclk_mhz(Some(0x20), None), None);
}

// --- AMD P-state decodes (ADR-049) ------------------------------------------

/// A documented family-17h P-state word: PstateEn(63) | CpuVid(21:14) <<
/// CpuDfsId(13:8) << CpuFid(7:0). NOT a host capture.
fn pstate(fid: u64, dfs_id: u64, vid: u64) -> u64 {
    (1 << 63) | (vid << 14) | (dfs_id << 8) | fid
}

#[test]
fn decode_amd_multiplier_follows_the_ppr_fid_over_dfs_times_two() {
    // CoreCOF = (CpuFid/CpuDfsId)*200 MHz -> multiplier = fid/did × 2
    // (e.g. fid 96, dfs 4 -> 48× -> 4.8 GHz).
    assert_eq!(decode_amd_multiplier(Some(pstate(96, 4, 0x40))), Some(48.0));
    assert_eq!(decode_amd_multiplier(Some(pstate(95, 5, 0x40))), Some(38.0));
    // PstateEn clear: the PPR marks the rest of the register invalid.
    assert_eq!(
        decode_amd_multiplier(Some(pstate(96, 4, 0x40) & !(1 << 63))),
        None,
    );
    // CpuDfsId 0 has no decode (libcpuid would divide by zero): honest null.
    assert_eq!(decode_amd_multiplier(Some(pstate(96, 0, 0x40))), None);
    // Out-of-envelope multipliers are garbage, not clamped (fid 255, dfs 1
    // -> 510×).
    assert_eq!(decode_amd_multiplier(Some(pstate(255, 1, 0x40))), None);
    // Register absent entirely.
    assert_eq!(decode_amd_multiplier(None), None);
}

#[test]
fn decode_amd_multipliers_select_current_pstate0_and_last_enabled() {
    let pstates = [
        Some(pstate(96, 4, 0x40)), // Pb0: max multiplier 48
        Some(pstate(80, 5, 0x6A)), // current: 32
        None,
        None,
        Some(pstate(36, 6, 0xA8)), // last enabled: 12
        None,
        None,
        Some(pstate(36, 6, 0xA8) & !(1 << 63)), // disabled: skipped by the scan
    ];
    // CurPstate = 1 selects the second register.
    assert_eq!(
        decode_amd_multiplier_current(Some(1), &pstates),
        Some(32.0),
        "80/5*2 = 32",
    );
    // Status register absent -> no current multiplier, but the min/max
    // scans do not depend on it.
    assert_eq!(decode_amd_multiplier_current(None, &pstates), None);
    assert_eq!(decode_amd_multiplier_max(&pstates), Some(48.0), "Pb0 first");
    // The min scan starts at P-state 7 and takes the first enabled one —
    // here index 7 is present but disabled, so index 4 wins.
    assert_eq!(decode_amd_multiplier_min(&pstates), Some(12.0));
    // A block with no enabled register decodes no minimum.
    let disabled = [None, None, None, None, None, None, None, Some(0)];
    assert_eq!(decode_amd_multiplier_min(&disabled), None);
    assert_eq!(decode_amd_multiplier_max(&disabled), None);
}

#[test]
fn decode_amd_vcore_uses_the_svi2_vid_table_and_rejects_out_of_envelope() {
    let pstates = [Some(pstate(80, 5, 0x6A)); PSTATE_REGISTERS];
    // V = 1.550 − 0.00625 × CpuVid (BKDG family 15h p.50); 0x6A = 106 ->
    // 0.8875 V. The step is not a dyadic rational, so compare with epsilon.
    let volts = decode_amd_vcore_v(Some(0), &pstates).expect("populated VID");
    assert!((volts - 0.8875).abs() < 1e-6, "got {volts}");
    // VID 0x00 is the top of the SVI2 table: exactly the base 1.550 V.
    let top = decode_amd_vcore_v(Some(0), &[Some(pstate(80, 5, 0)); PSTATE_REGISTERS]);
    assert_eq!(top, Some(1.550));
    // VID 0xFF decodes below zero volts: garbage, not clamped.
    let absurd = decode_amd_vcore_v(Some(0), &[Some(pstate(80, 5, 0xFF)); PSTATE_REGISTERS]);
    assert_eq!(absurd, None);
    // The current P-state's enable bit gates the decode; an absent status or
    // register stays null.
    let mut disabled = [Some(pstate(80, 5, 0x6A)); PSTATE_REGISTERS];
    disabled[0] = disabled[0].map(|value| value & !(1 << 63));
    assert_eq!(decode_amd_vcore_v(Some(0), &disabled), None);
    assert_eq!(decode_amd_vcore_v(None, &pstates), None);
    assert_eq!(decode_amd_vcore_v(Some(3), &[None; PSTATE_REGISTERS]), None,);
}

#[test]
fn decode_reading_routes_by_the_family_gate_and_keeps_amd_temperature_null() {
    let raw = RawRegisters {
        pstate_status: Some(1),
        pstates: [
            Some(pstate(96, 4, 0x40)),
            Some(pstate(80, 5, 0x6A)),
            None,
            None,
            Some(pstate(36, 6, 0xA8)),
            None,
            None,
            None,
        ],
        ..RawRegisters::default()
    };
    let zen = CpuIdentity {
        family: Some(0x19),
        bclk_mhz: None, // AMD does not enumerate CPUID 0x16: honest null.
    };
    let reading = decode_reading(5, &raw, &zen);
    assert_eq!(reading.cpu, 5);
    assert_eq!(reading.bclk_mhz, None);
    assert_eq!(
        reading.temperature_c, None,
        "no MSR-indexed AMD temperature exists (ADR-049)",
    );
    assert_eq!(reading.multiplier, Some(32.0));
    assert_eq!(reading.multiplier_min, Some(12.0));
    assert_eq!(reading.multiplier_max, Some(48.0));
    // The SVI2 step is not a dyadic rational: compare the volts separately.
    let volts = reading.vcore_v.expect("SVI2 VID decodes");
    assert!((volts - 0.8875).abs() < 1e-6, "got {volts}");
    // Families outside the verified window decode nothing, even with the
    // AMD register block present: Zen 5 (0x1A) and pre-Zen (0xF) have
    // different/unverified layouts.
    for family in [Some(0x1Au32), Some(0xFu32), Some(6u32)] {
        let row = decode_reading(
            5,
            &raw,
            &CpuIdentity {
                family,
                bclk_mhz: None,
            },
        );
        assert_eq!(
            row.multiplier, None,
            "family {family:?} must not AMD-decode"
        );
        assert_eq!(row.vcore_v, None);
    }
}

// --- register-read error mapping -------------------------------------------

#[test]
fn map_register_error_treats_no_data_as_absent_and_everything_else_as_failure() {
    // The driver reports an unimplemented register as EIO (errno 5).
    assert_eq!(
        map_register_error::<u64>(&io::Error::from_raw_os_error(5)),
        Ok(None),
        "EIO means the register is not implemented — the field stays null",
    );
    // Fixture trees model absence as end-of-file.
    assert_eq!(
        map_register_error::<u64>(&io::Error::new(io::ErrorKind::UnexpectedEof, "eof")),
        Ok(None),
    );
    // Any other I/O failure is a typed read failure for the whole sweep.
    assert_eq!(
        map_register_error::<u64>(&io::Error::from_raw_os_error(11)),
        Err(ErrorKindJson::ReadFailed),
    );
}

// --- fs walk over fixture /dev/cpu trees -----------------------------------

#[test]
fn walk_decodes_nodes_sorted_by_index_and_ignores_non_node_entries() {
    let root = fixture_root("sorted");
    fs::create_dir_all(&root).unwrap();
    write_msr_node(&root, 2, &documented_register_set());
    write_msr_node(&root, 0, &documented_register_set());
    // Not MSR nodes: non-numeric directory names (the kernel layout uses
    // bare numbers; anything else is ignored).
    fs::create_dir_all(root.join("cpu")).unwrap();
    fs::create_dir_all(root.join("cpu0")).unwrap();
    fs::create_dir_all(root.join("cpuidle")).unwrap();

    let ReadOutcome::Packages { packages } = collect_msr_readings(&root) else {
        panic!("fixture walk must succeed");
    };
    let cpus: Vec<u32> = packages.iter().map(|package| package.cpu).collect();
    assert_eq!(cpus, [0, 2], "nodes are sorted by their numeric suffix");
    assert_eq!(packages[0].temperature_c, Some(58.0));
    assert_eq!(packages[0].multiplier, Some(45.0));
    assert_eq!(packages[0].multiplier_min, Some(8.0));
    assert_eq!(packages[0].multiplier_max, Some(55.0));
    assert_eq!(packages[0].vcore_v, Some(1.21875));
}

#[test]
fn walk_caps_enumeration_at_the_node_ceiling() {
    let root = fixture_root("cap");
    fs::create_dir_all(&root).unwrap();
    write_msr_node(&root, 0, &[]);
    write_msr_node(&root, 1024, &documented_register_set());
    let ReadOutcome::Packages { packages } = collect_msr_readings(&root) else {
        panic!("fixture walk must succeed");
    };
    assert_eq!(
        packages.iter().map(|p| p.cpu).collect::<Vec<_>>(),
        [0],
        "nodes with N >= 1024 are ignored",
    );
}

#[test]
fn walk_reports_a_node_without_implemented_registers_as_all_nulls() {
    let root = fixture_root("absent");
    fs::create_dir_all(&root).unwrap();
    // A zero-register node encodes to an empty msr file: every register
    // offset is past end-of-file, the fixture stand-in for the driver's EIO
    // "register not implemented".
    write_msr_node(&root, 0, &[]);

    let ReadOutcome::Packages { packages } = collect_msr_readings(&root) else {
        panic!("fixture walk must succeed");
    };
    assert_eq!(packages.len(), 1);
    let reading = &packages[0];
    assert_eq!(reading.cpu, 0);
    assert_eq!(reading.temperature_c, None);
    assert_eq!(reading.multiplier, None);
    assert_eq!(reading.vcore_v, None);
}

#[test]
fn walk_missing_root_is_no_msr_and_empty_root_is_honest_success() {
    let missing = fixture_root("missing");
    match collect_msr_readings(&missing) {
        ReadOutcome::Error(error) => {
            assert_eq!(error.kind, ErrorKindJson::NoMsr);
            assert_eq!(error.kind.exit_code(), 3);
        }
        ReadOutcome::Packages { .. } => panic!("a missing /dev/cpu is no_msr"),
    }

    let empty = fixture_root("empty");
    fs::create_dir_all(&empty).unwrap();
    match collect_msr_readings(&empty) {
        ReadOutcome::Packages { packages } => {
            assert!(packages.is_empty(), "no nodes is an honest empty list");
        }
        ReadOutcome::Error(error) => panic!("an empty /dev/cpu is not an error: {error:?}"),
    }
}

#[test]
fn walk_skips_a_node_directory_without_an_msr_file() {
    let root = fixture_root("no_file");
    fs::create_dir_all(root.join("0")).unwrap();
    let ReadOutcome::Packages { packages } = collect_msr_readings(&root) else {
        panic!("fixture walk must succeed");
    };
    assert!(
        packages.is_empty(),
        "a numeric node directory without an msr file contributes no row",
    );
}

#[test]
fn walk_enumerates_the_bclk_from_the_read_only_cpuid_node() {
    let root = fixture_root("cpuid_intel");
    fs::create_dir_all(&root).unwrap();
    // Leaf 0: max standard leaf 0x20 (leaf 0x16 is enumerated); leaf 0x16:
    // ECX = 100 MHz — the SDM Bus (Reference) Frequency. The two leaves do
    // not overlap in a plain file image; leaf 1 is intentionally absent
    // (family stays unknown, which keeps the Intel register set selected).
    write_cpuid_node(&root, 0, &[(0x0, [0x20, 0, 0, 0]), (0x16, [0, 0, 100, 0])]);
    write_msr_node(&root, 0, &documented_register_set());

    let ReadOutcome::Packages { packages } = collect_msr_readings(&root) else {
        panic!("fixture walk must succeed");
    };
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].bclk_mhz, Some(100.0));
    assert_eq!(packages[0].temperature_c, Some(58.0));
}

#[test]
fn walk_without_leaf_0x16_support_keeps_bclk_null() {
    let root = fixture_root("cpuid_old");
    fs::create_dir_all(&root).unwrap();
    // A CPU whose max standard leaf stops before 0x16 aliases the leaf to
    // leaf-0 data (vendor ASCII); the gate must keep the field null even
    // when ECX-shaped bytes sit at the offset.
    write_cpuid_node(&root, 0, &[(0x0, [0x10, 0, 0, 0]), (0x16, [0, 0, 100, 0])]);
    write_msr_node(&root, 0, &documented_register_set());

    let ReadOutcome::Packages { packages } = collect_msr_readings(&root) else {
        panic!("fixture walk must succeed");
    };
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].bclk_mhz, None);
    assert_eq!(packages[0].multiplier, Some(45.0));
}

#[test]
fn amd_pstate_addresses_span_the_consecutive_block_the_ppr_defines() {
    assert_eq!(amd_pstate_address(0), 0xC001_0064);
    assert_eq!(amd_pstate_address(7), 0xC001_006B);
    for index in 0..PSTATE_REGISTERS {
        assert_eq!(amd_pstate_address(index), 0xC001_0064 + index as u64);
    }
}

#[test]
fn walk_switches_the_register_set_when_the_family_gate_selects_amd() {
    let root = fixture_root("cpuid_amd_route");
    fs::create_dir_all(&root).unwrap();
    // Leaf 1 EAX = 0x00800F10: base family 0xF + extended 0x8 = Zen (0x17).
    write_cpuid_node(&root, 0, &[(0x1, [0x0080_0F10, 0, 0, 0])]);
    // The msr node carries Intel-register bytes at Intel offsets; the AMD
    // sweep reads only the P-state block (~2 GiB, past end-of-file here), so
    // the row must be honest nulls — decoding the foreign register set would
    // be the fabrication this test guards against.
    write_msr_node(&root, 0, &documented_register_set());

    let ReadOutcome::Packages { packages } = collect_msr_readings(&root) else {
        panic!("fixture walk must succeed");
    };
    assert_eq!(packages.len(), 1);
    let reading = &packages[0];
    assert_eq!(reading.bclk_mhz, None);
    assert_eq!(reading.temperature_c, None);
    assert_eq!(reading.multiplier, None);
    assert_eq!(reading.multiplier_min, None);
    assert_eq!(reading.multiplier_max, None);
    assert_eq!(reading.vcore_v, None);
}

#[test]
fn walk_reads_and_decodes_the_amd_pstate_block() {
    let root = fixture_root("cpuid_amd_read");
    fs::create_dir_all(&root).unwrap();
    // Leaf 1 EAX = 0x00A00F10: Zen 3/4 (0x19).
    write_cpuid_node(&root, 0, &[(0x1, [0x00A0_0F10, 0, 0, 0])]);
    // CurPstate = 7 and one P-state word at address 0xC001006B — the only
    // faithful multi-word image for the consecutive-address block (the
    // status word at 0xC0010063..0xC001006A and P-state 7 at
    // 0xC001006B..0xC0010072 do not overlap; a plain file cannot hold the
    // interleaved-in-offset terms the real device reads independently).
    // fid 48, dfs 4 -> multiplier 24; vid 0x40 -> 1.550 - 0.4 = 1.15 V.
    write_amd_msr_node(&root, 0, 7, &[(7, pstate(48, 4, 0x40))]);

    let ReadOutcome::Packages { packages } = collect_msr_readings(&root) else {
        panic!("fixture walk must succeed");
    };
    assert_eq!(packages.len(), 1);
    let reading = &packages[0];
    assert_eq!(reading.cpu, 0);
    assert_eq!(reading.bclk_mhz, None, "AMD does not enumerate CPUID 0x16");
    assert_eq!(
        reading.temperature_c, None,
        "no MSR-indexed AMD temperature"
    );
    assert_eq!(reading.multiplier, Some(24.0), "current pstate 7: 48/4*2");
    assert_eq!(
        reading.multiplier_min,
        Some(24.0),
        "the scan starts at pstate 7"
    );
    assert_eq!(
        reading.multiplier_max, None,
        "P-state 0 is an unpopulated zero word: PstateEn clear",
    );
    let volts = reading.vcore_v.expect("SVI2 VID of the current pstate");
    assert!((volts - 1.15).abs() < 1e-6, "got {volts}");
}

#[test]
fn error_classification_maps_the_contract_kinds() {
    // Root classification: NotFound → no_msr, EACCES → permission_denied,
    // anything else → open_failed. Synthesized errno values keep this
    // deterministic regardless of the lane's euid.
    assert_eq!(
        classify_root_error(&io::Error::from_raw_os_error(2), Path::new("/dev/cpu")).kind,
        ErrorKindJson::NoMsr,
    );
    assert_eq!(
        classify_root_error(&io::Error::from_raw_os_error(13), Path::new("/dev/cpu")).kind,
        ErrorKindJson::PermissionDenied,
    );
    assert_eq!(
        classify_root_error(&io::Error::from_raw_os_error(5), Path::new("/dev/cpu")).kind,
        ErrorKindJson::OpenFailed,
    );
    // Node-open classification: NotFound → skip, EACCES → permission_denied,
    // anything else → open_failed.
    let node = Path::new("/dev/cpu/0/msr");
    assert_eq!(
        classify_open_error(&io::Error::from_raw_os_error(2), node),
        None
    );
    let denied = classify_open_error(&io::Error::from_raw_os_error(13), node)
        .expect("EACCES is a typed error");
    assert_eq!(denied.kind, ErrorKindJson::PermissionDenied);
    let failed = classify_open_error(&io::Error::from_raw_os_error(21), node)
        .expect("EISDIR is a typed error");
    assert_eq!(failed.kind, ErrorKindJson::OpenFailed);
}
