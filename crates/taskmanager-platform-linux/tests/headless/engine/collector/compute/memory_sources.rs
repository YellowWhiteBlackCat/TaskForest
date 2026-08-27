use super::memory_udev::parse_udev_memory_properties;
use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct FixtureDir(PathBuf);

impl FixtureDir {
    fn new() -> Self {
        let path = crate::test_support::repo_temp_dir().join(format!(
            "taskmanager-memory-provenance-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create memory provenance fixture");
        Self(path)
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn complete_zero_source_is_available_not_inferred_failed() {
    let fixture = FixtureDir::new();
    let meminfo = fixture.0.join("meminfo");
    fs::write(
        &meminfo,
        REQUIRED_MEMINFO_FIELDS
            .iter()
            .map(|field| format!("{field}: 0 kB"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .expect("write zero meminfo fixture");

    let observation = observe_meminfo_at(&meminfo);

    assert_eq!(observation.fields.get("Cached"), Some(&0));
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
}

#[test]
fn missing_required_meminfo_fields_are_partial_not_zero_success() {
    let fields = parse_meminfo_lines("Cached: 0 kB\nBuffers: 8 kB\n");
    let observed = REQUIRED_MEMINFO_FIELDS
        .iter()
        .filter(|field| fields.contains_key(**field))
        .count();
    let mut failures = FailureSummary::default();
    failures.record(FailureKind::ProviderFault);
    let status = source_status(MEMINFO_PROVIDER, observed, true, &failures);
    assert_eq!(fields.get("Cached"), Some(&0));
    assert_eq!(
        status.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
}

#[test]
fn optional_zfs_meminfo_field_parses_without_joining_the_required_set() {
    let fixture = FixtureDir::new();
    let meminfo = fixture.0.join("meminfo-zfs");
    fs::write(
        &meminfo,
        format!(
            "{}\nZfs: 512000 kB\n",
            REQUIRED_MEMINFO_FIELDS
                .iter()
                .map(|field| format!("{field}: 1 kB"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
    .expect("write zfs meminfo fixture");

    let zfs_host = observe_meminfo_at(&meminfo);
    assert_eq!(zfs_host.fields.get("Zfs"), Some(&(512_000 * 1024)));
    assert_eq!(zfs_host.status.outcome, SourceOutcome::Available);

    // A host without the field stays a fully successful observation: the
    // ARC is optional per-kernel, its absence is typed absence.
    let plain = fixture.0.join("meminfo-plain");
    fs::write(
        &plain,
        REQUIRED_MEMINFO_FIELDS
            .iter()
            .map(|field| format!("{field}: 1 kB"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .expect("write plain meminfo fixture");
    let non_zfs = observe_meminfo_at(&plain);
    assert_eq!(non_zfs.fields.get("Zfs"), None);
    assert_eq!(non_zfs.status.outcome, SourceOutcome::Available);

    // A malformed value is dropped to typed absence, never zero.
    let garbage = fixture.0.join("meminfo-garbage");
    fs::write(
        &garbage,
        format!(
            "{}\nZfs: not-a-number kB\n",
            REQUIRED_MEMINFO_FIELDS
                .iter()
                .map(|field| format!("{field}: 1 kB"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
    .expect("write garbage meminfo fixture");
    let malformed = observe_meminfo_at(&garbage);
    assert_eq!(malformed.fields.get("Zfs"), None);
}

#[test]
fn absent_compression_features_are_authoritative_empty() {
    let fixture = FixtureDir::new();
    let block_root = fixture.0.join("block");
    fs::create_dir_all(&block_root).expect("create empty block fixture");

    let observation = observe_compressed_swap_at(
        &block_root,
        &fixture.0.join("swaps"),
        &fixture.0.join("zswap-enable"),
    );

    assert_eq!(observation.status.outcome, SourceOutcome::Empty);
    assert_eq!(observation.status.item_count, 0);
    assert_eq!(observation.zram_total_bytes, None);
    assert_eq!(observation.zram_swap_used_bytes, None);
    assert_eq!(observation.zswap_enabled, None);
}

#[test]
fn dmi_values_and_status_share_one_sysfs_observation() {
    let fixture = FixtureDir::new();
    let dmi = fixture.0.join("dmi");
    fs::create_dir_all(&dmi).expect("create DMI fixture");
    fs::write(dmi.join("memory_speed_mhz"), "3200\n").expect("write memory speed");
    fs::write(dmi.join("memory_slots_used"), "0\n").expect("write used slots");
    fs::write(dmi.join("memory_slots_total"), "2\n").expect("write total slots");

    let observation = observe_dmi_memory_at(
        [&dmi, &fixture.0.join("fallback")],
        &fixture.0.join("entries"),
        &fixture.0.join("edac"),
    );

    assert_eq!(observation.speed_mhz, Some(3200));
    assert_eq!(observation.slots_used, Some(0));
    assert_eq!(observation.slots_total, Some(2));
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
    assert_eq!(observation.status.item_count, 3);
    assert!(observation.receipts.is_empty());
}

#[test]
fn dmi_failed_field_keeps_its_exact_receipt_while_siblings_stay_clean() {
    let fixture = FixtureDir::new();
    let dmi = fixture.0.join("dmi");
    fs::create_dir_all(&dmi).expect("create DMI fixture");
    fs::write(dmi.join("memory_speed_mhz"), "not-a-number\n").expect("write bad speed");
    fs::write(dmi.join("memory_slots_used"), "0\n").expect("write used slots");
    fs::write(dmi.join("memory_slots_total"), "2\n").expect("write total slots");

    let observation = observe_dmi_memory_at(
        [&dmi, &fixture.0.join("fallback")],
        &fixture.0.join("entries"),
        &fixture.0.join("edac"),
    );

    assert_eq!(observation.speed_mhz, None);
    assert_eq!(observation.slots_used, Some(0));
    assert_eq!(observation.slots_total, Some(2));
    assert_eq!(
        observation.status.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(
        observation.receipts,
        BTreeMap::from([(DMI_SPEED_FIELD, FailureKind::ProviderFault)])
    );
}

#[test]
fn type17_raw_failure_marks_both_dimm_fields_without_poisoning_sysfs_fields() {
    let fixture = FixtureDir::new();
    let dmi = fixture.0.join("dmi");
    fs::create_dir_all(&dmi).expect("create DMI fixture");
    fs::write(dmi.join("memory_speed_mhz"), "3200\n").expect("write memory speed");
    let entries = fixture.0.join("entries");
    fs::create_dir_all(entries.join("17-0")).expect("create type17 fixture");
    fs::write(entries.join("17-0").join("raw"), b"truncated").expect("write bad raw");

    let observation = observe_dmi_memory_at(
        [&dmi, &fixture.0.join("fallback")],
        &entries,
        &fixture.0.join("edac"),
    );

    assert_eq!(observation.speed_mhz, Some(3200));
    assert_eq!(
        observation.receipts.get(DMI_DIMM_SIZE_FIELD),
        Some(&FailureKind::ProviderFault)
    );
    assert_eq!(
        observation.receipts.get(DMI_DIMM_SPEED_FIELD),
        Some(&FailureKind::ProviderFault)
    );
    assert!(!observation.receipts.contains_key(DMI_SPEED_FIELD));
}

#[test]
fn zram_used_failure_is_receipted_without_masking_zswap_success() {
    let fixture = FixtureDir::new();
    let zram = fixture.0.join("block").join("zram0");
    fs::create_dir_all(&zram).expect("create zram fixture");
    fs::write(zram.join("disksize"), "4096\n").expect("write zram size");
    let swaps = fixture.0.join("swaps");
    fs::write(
        &swaps,
        "Filename Type Size Used Priority\n/dev/zram0 partition 4 invalid 100\n",
    )
    .expect("write malformed swaps fixture");
    fs::write(fixture.0.join("zswap-enable"), "y\n").expect("write zswap fixture");

    let observation = observe_compressed_swap_at(
        &fixture.0.join("block"),
        &swaps,
        &fixture.0.join("zswap-enable"),
    );

    assert_eq!(observation.zram_total_bytes, Some(4096));
    assert_eq!(observation.zswap_enabled, Some(true));
    assert_eq!(observation.zram_swap_used_bytes, None);
    assert_eq!(
        observation.receipts,
        BTreeMap::from([(ZRAM_USED_FIELD, FailureKind::ProviderFault)])
    );
}

#[test]
fn malformed_zram_swap_counter_is_not_reported_as_successful_zero() {
    let fixture = FixtureDir::new();
    let zram = fixture.0.join("block").join("zram0");
    fs::create_dir_all(&zram).expect("create zram fixture");
    fs::write(zram.join("disksize"), "4096\n").expect("write zram size");
    let swaps = fixture.0.join("swaps");
    fs::write(
        &swaps,
        "Filename Type Size Used Priority\n/dev/zram0 partition 4 invalid 100\n",
    )
    .expect("write malformed swaps fixture");

    let observation = observe_compressed_swap_at(
        &fixture.0.join("block"),
        &swaps,
        &fixture.0.join("zswap-enable"),
    );

    assert_eq!(observation.zram_total_bytes, Some(4096));
    assert_eq!(observation.zram_swap_used_bytes, None);
    assert_eq!(
        observation.status.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
}

#[test]
fn zram_mm_stat_sums_across_devices_into_compression_facts() {
    let fixture = FixtureDir::new();
    let block = fixture.0.join("block");
    for (device, line) in [
        (
            "zram0",
            "3221225472 1073741824 1207959552 0 2147483648 4096 8192\n",
        ),
        (
            "zram1",
            "1073741824 536870912 603979776 0 1073741824 2048 4096\n",
        ),
    ] {
        let zram = block.join(device);
        fs::create_dir_all(&zram).expect("create zram fixture");
        fs::write(zram.join("disksize"), "4294967296\n").expect("write zram size");
        fs::write(zram.join("mm_stat"), line).expect("write mm_stat fixture");
    }
    let swaps = fixture.0.join("swaps");
    fs::write(
        &swaps,
        "Filename Type Size Used Priority\n/dev/zram0 partition 4194304 1048576 100\n",
    )
    .expect("write swaps fixture");

    let observation = observe_compressed_swap_at(&block, &swaps, &fixture.0.join("zswap-enable"));

    assert_eq!(
        observation.zram_original_bytes,
        Some(3_221_225_472 + 1_073_741_824)
    );
    assert_eq!(
        observation.zram_compressed_bytes,
        Some(1_073_741_824 + 536_870_912)
    );
    assert_eq!(
        observation.zram_memory_used_bytes,
        Some(1_207_959_552 + 603_979_776)
    );
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
    assert!(observation.receipts.is_empty());

    // The aggregate observations carry the sums and the guarded ratio.
    let (_, compression) = assemble_module_and_compression_observations(
        &DmiMemoryObservation {
            speed_mhz: None,
            slots_used: None,
            slots_total: None,
            module_types: Vec::new(),
            module_manufacturers: Vec::new(),
            module_form_factors: Vec::new(),
            status: SourceStatus {
                provider: ProviderId::borrowed("linux.telemetry.memory.dmi"),
                outcome: SourceOutcome::Empty,
                item_count: 0,
            },
            receipts: BTreeMap::new(),
        },
        &observation,
        10,
    );
    assert_eq!(
        compression
            .compressed_swap_original_bytes
            .current_value()
            .copied(),
        Some(3_221_225_472 + 1_073_741_824)
    );
    assert_eq!(
        compression
            .compressed_swap_compressed_bytes
            .current_value()
            .copied(),
        Some(1_073_741_824 + 536_870_912)
    );
    assert_eq!(
        compression
            .compressed_swap_memory_used_bytes
            .current_value()
            .copied(),
        Some(1_207_959_552 + 603_979_776)
    );
    assert_eq!(
        compression.compression_ratio(),
        Some(
            (3_221_225_472u64 + 1_073_741_824u64) as f32
                / (1_073_741_824u64 + 536_870_912u64) as f32,
        )
    );
}

#[test]
fn short_or_garbage_mm_stat_receipts_fields_without_faking_zero_sums() {
    // A truncated line parses its leading fields but cannot yield an honest
    // per-device triple, so the device contributes nothing; only the fields
    // that actually failed carry receipts.
    let fixture = FixtureDir::new();
    let zram = fixture.0.join("block").join("zram0");
    fs::create_dir_all(&zram).expect("create zram fixture");
    fs::write(zram.join("disksize"), "4294967296\n").expect("write zram size");
    fs::write(zram.join("mm_stat"), "3221225472 1073741824\n").expect("write short mm_stat");
    let swaps = fixture.0.join("swaps");
    fs::write(
        &swaps,
        "Filename Type Size Used Priority\n/dev/zram0 partition 4194304 1048576 100\n",
    )
    .expect("write swaps fixture");

    let short =
        observe_compressed_swap_at(&fixture.0.join("block"), &swaps, &fixture.0.join("zswap"));
    assert_eq!(short.zram_original_bytes, None);
    assert_eq!(short.zram_compressed_bytes, None);
    assert_eq!(short.zram_memory_used_bytes, None);
    assert_eq!(
        short.status.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(
        short.receipts,
        BTreeMap::from([(ZRAM_MEMORY_USED_FIELD, FailureKind::ProviderFault)])
    );

    // A fully garbage line receipts every mm_stat field.
    let fixture = FixtureDir::new();
    let zram = fixture.0.join("block").join("zram1");
    fs::create_dir_all(&zram).expect("create zram fixture");
    fs::write(zram.join("disksize"), "4294967296\n").expect("write zram size");
    fs::write(zram.join("mm_stat"), "abc def ghi\n").expect("write garbage mm_stat");
    let swaps = fixture.0.join("swaps");
    fs::write(
        &swaps,
        "Filename Type Size Used Priority\n/dev/zram1 partition 4194304 1048576 100\n",
    )
    .expect("write swaps fixture");

    let garbage =
        observe_compressed_swap_at(&fixture.0.join("block"), &swaps, &fixture.0.join("zswap"));
    assert_eq!(garbage.zram_original_bytes, None);
    assert_eq!(garbage.zram_compressed_bytes, None);
    assert_eq!(garbage.zram_memory_used_bytes, None);
    assert_eq!(
        garbage.status.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    for field in [
        ZRAM_ORIGINAL_FIELD,
        ZRAM_COMPRESSED_FIELD,
        ZRAM_MEMORY_USED_FIELD,
    ] {
        assert_eq!(
            garbage.receipts.get(field),
            Some(&FailureKind::ProviderFault),
            "field {field} must carry its own receipt"
        );
    }
}

#[test]
fn absent_mm_stat_stays_a_typed_absence_not_a_failure() {
    let fixture = FixtureDir::new();
    let zram = fixture.0.join("block").join("zram0");
    fs::create_dir_all(&zram).expect("create zram fixture");
    fs::write(zram.join("disksize"), "4294967296\n").expect("write zram size");
    let swaps = fixture.0.join("swaps");
    fs::write(
        &swaps,
        "Filename Type Size Used Priority\n/dev/zram0 partition 4194304 1048576 100\n",
    )
    .expect("write swaps fixture");

    // No mm_stat file at all (older kernels/configs): the compression-depth
    // facts are absent with no failure receipt, while siblings stay healthy.
    let observation =
        observe_compressed_swap_at(&fixture.0.join("block"), &swaps, &fixture.0.join("zswap"));

    assert_eq!(observation.zram_total_bytes, Some(4_294_967_296));
    assert_eq!(observation.zram_swap_used_bytes, Some(1_048_576 * 1024));
    assert_eq!(observation.zram_original_bytes, None);
    assert_eq!(observation.zram_compressed_bytes, None);
    assert_eq!(observation.zram_memory_used_bytes, None);
    assert_eq!(observation.status.outcome, SourceOutcome::Available);
    assert!(observation.receipts.is_empty());
}

// ─── udev-database memory-device source (Mission Center parity, no sudo) ────

/// The udev DMI property shape this machine and Mission Center consume:
/// per-module `MEMORY_DEVICE_<n>_<PROP>` lines plus the array total.
const UDEV_FIXTURE: &str = "\
MEMORY_ARRAY_LOCATION=System Board Or Motherboard
MEMORY_ARRAY_NUM_DEVICES=4
MEMORY_DEVICE_0_PRESENT=1
MEMORY_DEVICE_0_SIZE=4096
MEMORY_DEVICE_0_TYPE=LPDDR5
MEMORY_DEVICE_0_MANUFACTURER=Samsung
MEMORY_DEVICE_0_FORM_FACTOR=Row of chips
MEMORY_DEVICE_0_SPEED_MTS=9600
MEMORY_DEVICE_0_CONFIGURED_SPEED_MTS=8533
MEMORY_DEVICE_0_RANK=2
MEMORY_DEVICE_0_LOCATOR=Controller0-ChannelA-DIMM0
MEMORY_DEVICE_1_PRESENT=1
MEMORY_DEVICE_1_SIZE=4096
MEMORY_DEVICE_1_TYPE=LPDDR5
MEMORY_DEVICE_1_MANUFACTURER=Samsung
MEMORY_DEVICE_1_FORM_FACTOR=Row of chips
MEMORY_DEVICE_1_LOCATOR=Controller0-ChannelA-DIMM1
MEMORY_DEVICE_2_PRESENT=0
MEMORY_DEVICE_2_LOCATOR=Controller0-ChannelB-DIMM0
MEMORY_DEVICE_3_PRESENT=0
MEMORY_DEVICE_3_LOCATOR=Controller0-ChannelB-DIMM1
";

#[test]
fn udev_properties_parse_present_modules_and_drop_absent_slots() {
    let devices = parse_udev_memory_properties(UDEV_FIXTURE).expect("udev fixture parses");

    assert_eq!(devices.slots_total, 4);
    assert_eq!(devices.modules.len(), 2, "absent slots must not count");
    assert_eq!(devices.modules[0].configured_speed_mts, Some(8533));
    assert_eq!(devices.modules[0].speed_mts, Some(9600));
    assert_eq!(devices.modules[0].module_type.as_deref(), Some("LPDDR5"));
    assert_eq!(devices.modules[0].manufacturer.as_deref(), Some("Samsung"));
    assert_eq!(
        devices.modules[0].form_factor.as_deref(),
        Some("Row of chips")
    );
    assert_eq!(devices.modules[0].size_mib, Some(4096));
    assert_eq!(devices.modules[0].rank, Some(2));
    assert_eq!(
        devices.modules[1].locator.as_deref(),
        Some("Controller0-ChannelA-DIMM1")
    );
}

#[test]
fn udev_properties_filter_out_of_spec_and_unspecified_labels() {
    let devices = parse_udev_memory_properties(
        "MEMORY_DEVICE_0_PRESENT=1\n\
         MEMORY_DEVICE_0_TYPE=<OUT OF SPEC>\n\
         MEMORY_DEVICE_0_MANUFACTURER=Not Specified\n\
         MEMORY_DEVICE_0_FORM_FACTOR=<OUT OF SPEC>\n",
    )
    .expect("fixture parses");
    assert_eq!(devices.modules[0].module_type, None);
    assert_eq!(devices.modules[0].manufacturer, None);
    assert_eq!(devices.modules[0].form_factor, None);
}

#[test]
fn udev_properties_without_any_memory_device_yield_none() {
    assert_eq!(
        parse_udev_memory_properties("MEMORY_ARRAY_LOCATION=System Board\n"),
        None,
        "a machine without udev memory properties must degrade to raw-DMI"
    );
}

#[test]
fn udev_properties_fall_back_to_module_count_for_slots_total() {
    let devices =
        parse_udev_memory_properties("MEMORY_DEVICE_0_PRESENT=1\nMEMORY_DEVICE_0_TYPE=DDR5\n")
            .expect("fixture parses");
    assert_eq!(devices.slots_total, 1);
}

#[test]
fn observe_dmi_memory_prefers_udev_speed_type_and_slots() {
    // A raw-DMI observation that found nothing: the udev merge must supply
    // speed, slots, and module types; raw values must NOT be overwritten when
    // they exist.
    let mut empty = DmiMemoryObservation {
        speed_mhz: None,
        slots_used: None,
        slots_total: None,
        module_types: Vec::new(),
        module_manufacturers: Vec::new(),
        module_form_factors: Vec::new(),
        status: SourceStatus {
            provider: ProviderId::borrowed("linux.telemetry.memory.dmi"),
            outcome: SourceOutcome::Empty,
            item_count: 0,
        },
        receipts: BTreeMap::new(),
    };
    let devices = parse_udev_memory_properties(UDEV_FIXTURE).expect("udev fixture parses");
    merge_udev_into_dmi(&mut empty, &devices);

    assert_eq!(
        empty.speed_mhz,
        Some(8533),
        "configured speed wins over max"
    );
    assert_eq!(empty.slots_total, Some(4));
    assert_eq!(empty.slots_used, Some(2));
    assert_eq!(empty.module_types, vec!["LPDDR5"]);
    assert_eq!(empty.module_manufacturers, vec!["Samsung"]);
    assert_eq!(empty.module_form_factors, vec!["Row of chips"]);

    // Existing raw-DMI values are authoritative and never downgraded by udev.
    let mut raw = DmiMemoryObservation {
        speed_mhz: Some(6400),
        slots_used: Some(2),
        slots_total: Some(4),
        module_types: Vec::new(),
        module_manufacturers: Vec::new(),
        module_form_factors: Vec::new(),
        status: SourceStatus {
            provider: ProviderId::borrowed("linux.telemetry.memory.dmi"),
            outcome: SourceOutcome::Available,
            item_count: 2,
        },
        receipts: BTreeMap::new(),
    };
    merge_udev_into_dmi(&mut raw, &devices);
    assert_eq!(raw.speed_mhz, Some(6400), "raw-DMI value must survive");
    assert_eq!(raw.module_types, vec!["LPDDR5"], "types still merge");

    let (modules, _compression) = assemble_module_and_compression_observations(
        &empty,
        &CompressedSwapObservation {
            zram_swap_used_bytes: None,
            zram_total_bytes: None,
            zswap_enabled: None,
            zram_original_bytes: None,
            zram_compressed_bytes: None,
            zram_memory_used_bytes: None,
            status: SourceStatus {
                provider: ProviderId::borrowed("linux.telemetry.memory.zram-zswap"),
                outcome: SourceOutcome::Empty,
                item_count: 0,
            },
            receipts: BTreeMap::new(),
        },
        10,
    );
    assert_eq!(
        modules.module_type.current_value().map(String::as_str),
        Some("LPDDR5")
    );
    assert_eq!(modules.speed_mhz.current_value().copied(), Some(8533));
}
