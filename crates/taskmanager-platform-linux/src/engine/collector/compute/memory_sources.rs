//! Independently fallible Linux memory telemetry source probes.

mod memory_udev;
use memory_udev::{merge_udev_into_dmi, observe_udev_memory_devices};

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

use taskmanager_platform_contract::{FailureKind, ProviderId, SourceOutcome, SourceStatus};

use super::super::parse_meminfo_lines;
use super::super::{MemoryCompressionObservations, MemoryModuleObservations, OptionalObservation};

const SYSINFO_PROVIDER: &str = "linux.telemetry.memory.sysinfo";
const MEMINFO_PROVIDER: &str = "linux.telemetry.memory.proc-meminfo";
const DMI_PROVIDER: &str = "linux.telemetry.memory.dmi";
const COMPRESSED_SWAP_PROVIDER: &str = "linux.telemetry.memory.zram-zswap";

/// Bound on the `udevadm info` database query. The udev database is local and
/// tiny; five seconds is far beyond any real latency and only guards a wedged
/// udev daemon.
pub(super) const UDEV_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

const REQUIRED_MEMINFO_FIELDS: [&str; 8] = [
    "Cached",
    "Buffers",
    "Active",
    "Inactive",
    "MemFree",
    "SReclaimable",
    "Committed_AS",
    "CommitLimit",
];

/// Field names for precise per-field failure receipts. A provider stays
/// `Partial` when one field fails; consumers must see the exact failing field
/// instead of a provider-wide reason.
pub(super) const DMI_SPEED_FIELD: &str = "dmi.speed_mhz";
pub(super) const DMI_SLOTS_USED_FIELD: &str = "dmi.slots_used";
pub(super) const DMI_SLOTS_TOTAL_FIELD: &str = "dmi.slots_total";
const DMI_DIMM_SIZE_FIELD: &str = "dmi.dimm_size";
const DMI_DIMM_SPEED_FIELD: &str = "dmi.dimm_speed";
const DMI_EDAC_SIZE_FIELD: &str = "dmi.edac_size";
pub(super) const ZRAM_TOTAL_FIELD: &str = "compression.zram_total_bytes";
pub(super) const ZRAM_USED_FIELD: &str = "compression.zram_swap_used_bytes";
pub(super) const ZSWAP_ENABLED_FIELD: &str = "compression.zswap_enabled";
pub(super) const ZRAM_ORIGINAL_FIELD: &str = "compression.zram_original_bytes";
pub(super) const ZRAM_COMPRESSED_FIELD: &str = "compression.zram_compressed_bytes";
pub(super) const ZRAM_MEMORY_USED_FIELD: &str = "compression.zram_memory_used_bytes";
/// Failures that do not belong to a single measured field.
const PROVIDER_LEVEL_FIELD: &str = "provider";

#[derive(Debug)]
pub(super) struct MeminfoObservation {
    pub fields: HashMap<String, u64>,
    pub status: SourceStatus,
}

#[derive(Debug)]
pub(super) struct DmiMemoryObservation {
    pub speed_mhz: Option<u32>,
    pub slots_used: Option<usize>,
    pub slots_total: Option<usize>,
    /// Distinct module technology labels (DMI type-17 "Type"), e.g.
    /// `["LPDDR5"]` or `["DDR5", "DDR4"]` for mixed populations. Populated by
    /// the privilege-free udev-database source when available.
    pub module_types: Vec<String>,
    /// Distinct module manufacturer labels, e.g. `["Samsung"]`.
    pub module_manufacturers: Vec<String>,
    /// Distinct module form-factor labels (SO-DIMM / DIMM / ...), with
    /// out-of-spec sentinels filtered out.
    pub module_form_factors: Vec<String>,
    pub status: SourceStatus,
    /// Exact failure per measured field, for consumers that need the precise
    /// receipt instead of the aggregate provider outcome.
    pub receipts: BTreeMap<&'static str, FailureKind>,
}

/// One parsed physical-module record from the world-readable udev database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UdevMemoryModule {
    /// `PRESENT` property; absent slots are dropped from the final list.
    pub present: bool,
    pub size_mib: Option<u64>,
    pub module_type: Option<String>,
    pub manufacturer: Option<String>,
    pub form_factor: Option<String>,
    /// `SPEED_MTS` (device maximum).
    pub speed_mts: Option<u32>,
    /// `CONFIGURED_SPEED_MTS` (what the platform actually runs).
    pub configured_speed_mts: Option<u32>,
    pub rank: Option<u32>,
    pub locator: Option<String>,
}

#[derive(Debug)]
pub(super) struct CompressedSwapObservation {
    pub zram_swap_used_bytes: Option<u64>,
    pub zram_total_bytes: Option<u64>,
    pub zswap_enabled: Option<bool>,
    /// Summed `mm_stat` `orig_data_size` across zram devices: the
    /// uncompressed size of the data currently held in the store.
    pub zram_original_bytes: Option<u64>,
    /// Summed `mm_stat` `compr_data_size`: that data after compression.
    pub zram_compressed_bytes: Option<u64>,
    /// Summed `mm_stat` `mem_used_total`: RAM consumed by the store.
    pub zram_memory_used_bytes: Option<u64>,
    pub status: SourceStatus,
    pub receipts: BTreeMap<&'static str, FailureKind>,
}

#[derive(Debug, Default)]
struct FailureSummary {
    /// Highest-priority failure per field, so one broken source never masks a
    /// healthy sibling field's success.
    fields: BTreeMap<&'static str, FailureKind>,
}

impl FailureSummary {
    fn record_field(&mut self, field: &'static str, failure: FailureKind) {
        if self
            .fields
            .get(field)
            .is_none_or(|current| priority(failure) > priority(*current))
        {
            self.fields.insert(field, failure);
        }
    }

    fn record(&mut self, failure: FailureKind) {
        self.record_field(PROVIDER_LEVEL_FIELD, failure);
    }

    fn aggregate(&self) -> Option<FailureKind> {
        self.fields
            .values()
            .copied()
            .max_by_key(|failure| priority(*failure))
    }
}

const fn priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::Unsupported => 3,
        FailureKind::IdentityChanged | FailureKind::Rejected => 1,
    }
}

fn classify_io(error: &io::Error) -> FailureKind {
    match error.kind() {
        io::ErrorKind::NotFound => FailureKind::Unsupported,
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        io::ErrorKind::TimedOut => FailureKind::TimedOut,
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => {
            FailureKind::TemporarilyUnavailable
        }
        _ => FailureKind::ProviderFault,
    }
}

/// Classify an I/O failure on a root-only SMBIOS node (the type-17 `raw` reads):
/// `PermissionDenied` becomes [`FailureKind::RequiresEscalation`] when the gate
/// confirms [`EscalationFeature::MemorySmbios`] is escalatable — the user can
/// grant it via the OS-native prompt, so the metric reads as "needs permission"
/// rather than a hard denial. Mirrors the Intel PMU path's
/// `classify_intel_pmu_open_failure`. Every other failure kind passes through.
fn classify_smbios_io(error: &io::Error) -> FailureKind {
    use taskmanager_escalation::{
        EscalationAvailability, EscalationFeature, PrivilegeGate, UnprivilegedGate,
    };
    let classified = classify_io(error);
    if classified == FailureKind::PermissionDenied
        && matches!(
            UnprivilegedGate.probe(EscalationFeature::MemorySmbios),
            EscalationAvailability::RequiresEscalation(_)
        )
    {
        FailureKind::RequiresEscalation
    } else {
        classified
    }
}

fn source_status(
    provider: &'static str,
    observed: usize,
    source_reached: bool,
    failures: &FailureSummary,
) -> SourceStatus {
    let aggregate = failures.aggregate();
    let outcome = if observed > 0 {
        aggregate.map_or(SourceOutcome::Available, SourceOutcome::Partial)
    } else if let Some(failure) = aggregate {
        SourceOutcome::Unavailable(failure)
    } else if source_reached {
        SourceOutcome::Empty
    } else {
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    };
    SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome,
        item_count: observed,
    }
}

pub(super) fn sysinfo_status() -> SourceStatus {
    // sysinfo has already completed its refresh before this provider is called
    // and does not expose per-domain I/O errors. Zero is a valid measurement,
    // so availability is based on the completed source call, never its values.
    SourceStatus {
        provider: ProviderId::borrowed(SYSINFO_PROVIDER),
        outcome: SourceOutcome::Available,
        item_count: 5,
    }
}

pub(super) fn observe_meminfo() -> MeminfoObservation {
    observe_meminfo_at(Path::new("/proc/meminfo"))
}

fn observe_meminfo_at(path: &Path) -> MeminfoObservation {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) => {
            let mut failures = FailureSummary::default();
            failures.record_field(PROVIDER_LEVEL_FIELD, classify_io(&error));
            return MeminfoObservation {
                fields: HashMap::new(),
                status: source_status(MEMINFO_PROVIDER, 0, false, &failures),
            };
        }
    };
    let fields = parse_meminfo_lines(&content);
    let observed = REQUIRED_MEMINFO_FIELDS
        .iter()
        .filter(|field| fields.contains_key(**field))
        .count();
    let mut failures = FailureSummary::default();
    if observed != REQUIRED_MEMINFO_FIELDS.len() {
        failures.record(FailureKind::ProviderFault);
    }
    MeminfoObservation {
        fields,
        status: source_status(MEMINFO_PROVIDER, observed, true, &failures),
    }
}

pub(super) fn observe_dmi_memory() -> DmiMemoryObservation {
    let mut observation = observe_dmi_memory_at(
        [
            Path::new("/sys/class/dmi/id"),
            Path::new("/sys/devices/virtual/dmi/id"),
        ],
        Path::new("/sys/firmware/dmi/entries"),
        Path::new("/sys/devices/system/edac/mc"),
    );
    // Privilege-free udev-database source: systemd udev (root) parses the DMI
    // at boot and caches per-module properties (`MEMORY_DEVICE_N_*`) in its
    // world-readable database; `udevadm info` queries it with no privileges.
    // Precedence: udev first (authoritative, privilege-free), raw DMI only
    // fills what udev could not provide. Absent udev (non-systemd / older
    // builtin) keeps the historical raw-DMI path untouched.
    if let Some(devices) = observe_udev_memory_devices() {
        merge_udev_into_dmi(&mut observation, &devices);
    }
    observation
}

/// Merge the udev-database facts into a raw-DMI observation: udev values win
/// (configured speed over maximum, exact slots), raw-DMI fills gaps only.
/// Provided fields lose their raw-DMI failure receipts, and a non-empty udev
/// contribution upgrades the source status (the udev database is
/// privilege-free and authoritative — an empty raw-DMI result must not mask
/// it). The provider identity stays `dmi` — udev is the mechanism by which the
/// DMI facts are obtained, not a separate capability. Pure so the precedence
/// rules are unit-testable without udevadm.
fn observe_dmi_memory_at(
    dmi_id_roots: [&Path; 2],
    dmi_entries_root: &Path,
    edac_root: &Path,
) -> DmiMemoryObservation {
    let mut failures = FailureSummary::default();
    let mut source_reached = false;
    let mut observed = 0usize;
    let mut speed_mhz = None;
    let mut slots_used = None;
    let mut slots_total = None;

    for root in dmi_id_roots {
        match fs::read_dir(root) {
            Ok(_) => {
                source_reached = true;
                for (speed_name, used_name, total_name) in [
                    (
                        "memory_speed_mhz",
                        "memory_slots_used",
                        "memory_slots_total",
                    ),
                    ("memory_speed", "slots_used", "slots_total"),
                ] {
                    if speed_mhz.is_none()
                        && let Some(value) = read_optional_u64(
                            &root.join(speed_name),
                            &mut failures,
                            DMI_SPEED_FIELD,
                        )
                    {
                        match u32::try_from(value) {
                            Ok(value) if value > 0 => {
                                speed_mhz = Some(value);
                                observed = observed.saturating_add(1);
                            }
                            Ok(_) => {}
                            Err(_) => {
                                failures.record_field(DMI_SPEED_FIELD, FailureKind::ProviderFault)
                            }
                        }
                    }
                    if slots_used.is_none()
                        && let Some(value) = read_optional_u64(
                            &root.join(used_name),
                            &mut failures,
                            DMI_SLOTS_USED_FIELD,
                        )
                    {
                        match usize::try_from(value) {
                            Ok(value) => {
                                slots_used = Some(value);
                                observed = observed.saturating_add(1);
                            }
                            Err(_) => failures
                                .record_field(DMI_SLOTS_USED_FIELD, FailureKind::ProviderFault),
                        }
                    }
                    if slots_total.is_none()
                        && let Some(value) = read_optional_u64(
                            &root.join(total_name),
                            &mut failures,
                            DMI_SLOTS_TOTAL_FIELD,
                        )
                    {
                        match usize::try_from(value) {
                            Ok(value) => {
                                slots_total = Some(value);
                                observed = observed.saturating_add(1);
                            }
                            Err(_) => failures
                                .record_field(DMI_SLOTS_TOTAL_FIELD, FailureKind::ProviderFault),
                        }
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => failures.record_field(DMI_SPEED_FIELD, classify_io(&error)),
        }
    }

    match fs::read_dir(dmi_entries_root) {
        Ok(entries) => {
            source_reached = true;
            let mut type17_count = 0usize;
            let mut type17_used = 0usize;
            let mut max_speed = 0u32;
            for entry in entries {
                let Ok(entry) = entry else {
                    failures.record_field(DMI_DIMM_SIZE_FIELD, FailureKind::ProviderFault);
                    failures.record_field(DMI_DIMM_SPEED_FIELD, FailureKind::ProviderFault);
                    continue;
                };
                if !entry.file_name().to_string_lossy().starts_with("17-") {
                    continue;
                }
                match fs::read(entry.path().join("raw")) {
                    Ok(bytes) if bytes.len() >= 23 => {
                        observed = observed.saturating_add(1);
                        type17_count = type17_count.saturating_add(1);
                        let size = u16::from_le_bytes([bytes[12], bytes[13]]);
                        if size > 0 && size != u16::MAX {
                            type17_used = type17_used.saturating_add(1);
                        }
                        let mut module_speed =
                            u32::from(u16::from_le_bytes([bytes[21], bytes[22]]));
                        if bytes.len() >= 34 {
                            let configured = u32::from(u16::from_le_bytes([bytes[32], bytes[33]]));
                            if configured > 0 && configured != u32::from(u16::MAX) {
                                module_speed = configured;
                            }
                        }
                        if module_speed > 0 && module_speed != u32::from(u16::MAX) {
                            max_speed = max_speed.max(module_speed);
                        }
                    }
                    Ok(_) => {
                        failures.record_field(DMI_DIMM_SIZE_FIELD, FailureKind::ProviderFault);
                        failures.record_field(DMI_DIMM_SPEED_FIELD, FailureKind::ProviderFault);
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        failures.record_field(DMI_DIMM_SIZE_FIELD, classify_smbios_io(&error));
                        failures.record_field(DMI_DIMM_SPEED_FIELD, classify_smbios_io(&error));
                    }
                }
            }
            if type17_count > 0 {
                slots_total.get_or_insert(type17_count);
                slots_used.get_or_insert(type17_used);
                if max_speed > 0 {
                    speed_mhz.get_or_insert(max_speed);
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            failures.record_field(DMI_DIMM_SIZE_FIELD, classify_io(&error));
            failures.record_field(DMI_DIMM_SPEED_FIELD, classify_io(&error));
        }
    }

    if slots_total.is_none() || slots_used.is_none() {
        match fs::read_dir(edac_root) {
            Ok(controllers) => {
                source_reached = true;
                let mut edac_total = 0usize;
                let mut edac_used = 0usize;
                for controller in controllers.flatten() {
                    if !controller.file_name().to_string_lossy().starts_with("mc") {
                        continue;
                    }
                    let dimms = match fs::read_dir(controller.path()) {
                        Ok(dimms) => dimms,
                        Err(error) => {
                            failures.record_field(DMI_EDAC_SIZE_FIELD, classify_io(&error));
                            continue;
                        }
                    };
                    for dimm in dimms.flatten() {
                        if !dimm.file_name().to_string_lossy().starts_with("dimm") {
                            continue;
                        }
                        edac_total = edac_total.saturating_add(1);
                        if let Some(size) = read_optional_u64(
                            &dimm.path().join("size"),
                            &mut failures,
                            DMI_EDAC_SIZE_FIELD,
                        ) {
                            observed = observed.saturating_add(1);
                            if size > 0 {
                                edac_used = edac_used.saturating_add(1);
                            }
                        }
                    }
                }
                if edac_total > 0 {
                    slots_total.get_or_insert(edac_total);
                    slots_used.get_or_insert(edac_used);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => failures.record_field(DMI_EDAC_SIZE_FIELD, classify_io(&error)),
        }
    }

    DmiMemoryObservation {
        speed_mhz,
        slots_used,
        slots_total,
        module_types: Vec::new(),
        module_manufacturers: Vec::new(),
        module_form_factors: Vec::new(),
        status: source_status(DMI_PROVIDER, observed, source_reached, &failures),
        receipts: failures.fields,
    }
}

pub(super) fn observe_compressed_swap() -> CompressedSwapObservation {
    observe_compressed_swap_at(
        Path::new("/sys/block"),
        Path::new("/proc/swaps"),
        Path::new("/sys/module/zswap/parameters/enabled"),
    )
}

fn observe_compressed_swap_at(
    block_root: &Path,
    swaps_path: &Path,
    zswap_path: &Path,
) -> CompressedSwapObservation {
    let mut failures = FailureSummary::default();
    let mut observed = 0usize;
    let mut source_reached = false;
    let mut zram_present = false;
    let mut zram_total = 0u64;
    let mut zram_total_observed = false;
    let mut zram_mm = (0u64, 0u64, 0u64);
    let mut zram_mm_observed = false;

    match fs::read_dir(block_root) {
        Ok(entries) => {
            source_reached = true;
            for entry in entries {
                let Ok(entry) = entry else {
                    failures.record_field(ZRAM_TOTAL_FIELD, FailureKind::ProviderFault);
                    continue;
                };
                if !entry.file_name().to_string_lossy().starts_with("zram") {
                    continue;
                }
                zram_present = true;
                if let Some(size) = read_optional_u64(
                    &entry.path().join("disksize"),
                    &mut failures,
                    ZRAM_TOTAL_FIELD,
                ) {
                    zram_total = zram_total.saturating_add(size);
                    zram_total_observed = true;
                }
                if let Some((orig, compr, mem_used)) =
                    read_zram_mm_stat(&entry.path().join("mm_stat"), &mut failures)
                {
                    zram_mm.0 = zram_mm.0.saturating_add(orig);
                    zram_mm.1 = zram_mm.1.saturating_add(compr);
                    zram_mm.2 = zram_mm.2.saturating_add(mem_used);
                    zram_mm_observed = true;
                }
            }
        }
        Err(error) => failures.record_field(ZRAM_TOTAL_FIELD, classify_io(&error)),
    }

    let zram_total_bytes = if zram_present && zram_total_observed {
        observed = observed.saturating_add(1);
        Some(zram_total)
    } else {
        None
    };
    let (zram_original_bytes, zram_compressed_bytes, zram_memory_used_bytes) =
        if zram_present && zram_mm_observed {
            observed = observed.saturating_add(3);
            (Some(zram_mm.0), Some(zram_mm.1), Some(zram_mm.2))
        } else {
            (None, None, None)
        };
    let zram_swap_used_bytes = if zram_present {
        match fs::read_to_string(swaps_path) {
            Ok(content) => {
                let mut used_kib = 0u64;
                let mut zram_rows = 0usize;
                let mut parsed_rows = 0usize;
                for line in content.lines().skip(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.first().is_none_or(|device| !device.contains("zram")) {
                        continue;
                    }
                    zram_rows = zram_rows.saturating_add(1);
                    if parts.len() < 4 {
                        failures.record_field(ZRAM_USED_FIELD, FailureKind::ProviderFault);
                        continue;
                    }
                    match parts[3].parse::<u64>() {
                        Ok(used) => {
                            parsed_rows = parsed_rows.saturating_add(1);
                            used_kib = used_kib.saturating_add(used);
                        }
                        Err(_) => {
                            failures.record_field(ZRAM_USED_FIELD, FailureKind::ProviderFault)
                        }
                    }
                }
                if zram_rows > 0 && parsed_rows == 0 {
                    None
                } else {
                    observed = observed.saturating_add(1);
                    Some(used_kib.saturating_mul(1024))
                }
            }
            Err(error) => {
                failures.record_field(ZRAM_USED_FIELD, classify_io(&error));
                None
            }
        }
    } else {
        None
    };

    let zswap_enabled = match fs::read_to_string(zswap_path) {
        Ok(raw) => {
            source_reached = true;
            match raw.trim().to_ascii_lowercase().as_str() {
                "y" | "1" | "true" => {
                    observed = observed.saturating_add(1);
                    Some(true)
                }
                "n" | "0" | "false" => {
                    observed = observed.saturating_add(1);
                    Some(false)
                }
                _ => {
                    failures.record_field(ZSWAP_ENABLED_FIELD, FailureKind::ProviderFault);
                    None
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            failures.record_field(ZSWAP_ENABLED_FIELD, classify_io(&error));
            None
        }
    };

    CompressedSwapObservation {
        zram_swap_used_bytes,
        zram_total_bytes,
        zswap_enabled,
        zram_original_bytes,
        zram_compressed_bytes,
        zram_memory_used_bytes,
        status: source_status(
            COMPRESSED_SWAP_PROVIDER,
            observed,
            source_reached,
            &failures,
        ),
        receipts: failures.fields,
    }
}

/// Parse one zram `mm_stat` file: whitespace-separated byte counters
/// (`orig_data_size compr_data_size mem_used_total mem_limit mem_used_max
/// same_pages pages_compacted …`). A missing file is a typed absence (older
/// kernels/configs); a short or non-numeric line is a per-field provider
/// fault — never a believable zero.
fn read_zram_mm_stat(path: &Path, failures: &mut FailureSummary) -> Option<(u64, u64, u64)> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            let failure = classify_io(&error);
            failures.record_field(ZRAM_ORIGINAL_FIELD, failure);
            failures.record_field(ZRAM_COMPRESSED_FIELD, failure);
            failures.record_field(ZRAM_MEMORY_USED_FIELD, failure);
            return None;
        }
    };
    parse_zram_mm_stat_fields(&raw, failures)
}

/// Pure-text half of [`read_zram_mm_stat`]: only the first three counters
/// feed the aggregate compression observations, and each missing or
/// non-numeric token records its own provider fault before the all-or-none
/// verdict.
fn parse_zram_mm_stat_fields(raw: &str, failures: &mut FailureSummary) -> Option<(u64, u64, u64)> {
    let mut tokens = raw.split_whitespace();
    let orig = parse_mm_stat_token(tokens.next(), ZRAM_ORIGINAL_FIELD, failures);
    let compr = parse_mm_stat_token(tokens.next(), ZRAM_COMPRESSED_FIELD, failures);
    let mem_used = parse_mm_stat_token(tokens.next(), ZRAM_MEMORY_USED_FIELD, failures);
    match (orig, compr, mem_used) {
        (Some(orig), Some(compr), Some(mem_used)) => Some((orig, compr, mem_used)),
        // A partial parse would mix per-device sums with fabricated zeros.
        _ => None,
    }
}

/// Fuzz-reachable seam over [`parse_zram_mm_stat_fields`], behind
/// `test-support` exactly like the procfs parser exports: arbitrary
/// `mm_stat`-shaped text — any field count, value range, or garbage bytes —
/// either yields all three counters or `None`, never a panic and never a
/// fabricated zero.
#[cfg(feature = "test-support")]
pub fn parse_zram_mm_stat(raw: &str) -> Option<(u64, u64, u64)> {
    parse_zram_mm_stat_fields(raw, &mut FailureSummary::default())
}

fn parse_mm_stat_token(
    token: Option<&str>,
    field: &'static str,
    failures: &mut FailureSummary,
) -> Option<u64> {
    let Some(token) = token else {
        failures.record_field(field, FailureKind::ProviderFault);
        return None;
    };
    match token.parse::<u64>() {
        Ok(value) => Some(value),
        Err(_) => {
            failures.record_field(field, FailureKind::ProviderFault);
            None
        }
    }
}

fn read_optional_u64(
    path: &Path,
    failures: &mut FailureSummary,
    field: &'static str,
) -> Option<u64> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            failures.record_field(field, classify_io(&error));
            return None;
        }
    };
    match raw.trim().parse::<u64>() {
        Ok(value) => Some(value),
        Err(_) => {
            failures.record_field(field, FailureKind::ProviderFault);
            None
        }
    }
}

/// Like the compute assembly's [`super::optional_from_source`], but a missing
/// field first consults the provider's exact per-field receipt so one field's
/// failure never contaminates a sibling field's success (or absence) with the
/// provider-wide reason.
pub(super) fn optional_from_source_with_receipt<T>(
    value: Option<T>,
    source: &SourceStatus,
    receipts: &BTreeMap<&'static str, FailureKind>,
    field: &'static str,
    now_ms: u64,
) -> OptionalObservation<T> {
    if value.is_none()
        && let Some(failure) = receipts.get(field)
    {
        return OptionalObservation::unavailable(*failure);
    }
    super::optional_from_source(value, source, now_ms)
}

/// Distinct module types joined for display, e.g. `"LPDDR5"` or
/// `"DDR4 / DDR5"`. An empty-but-confirmed list (udev reported modules with no
/// Type field) stays absent — a fabricated label would be a lie.
fn module_type_observation(dmi: &DmiMemoryObservation, now_ms: u64) -> OptionalObservation<String> {
    let joined = (!dmi.module_types.is_empty()).then(|| dmi.module_types.join(" / "));
    super::optional_from_source(joined, &dmi.status, now_ms)
}

/// Distinct module manufacturers joined for display, e.g. `"Samsung"`.
fn module_manufacturer_observation(
    dmi: &DmiMemoryObservation,
    now_ms: u64,
) -> OptionalObservation<String> {
    let joined =
        (!dmi.module_manufacturers.is_empty()).then(|| dmi.module_manufacturers.join(" / "));
    super::optional_from_source(joined, &dmi.status, now_ms)
}

/// Distinct module form factors joined for display, e.g. `"SO-DIMM"`.
fn module_form_factor_observation(
    dmi: &DmiMemoryObservation,
    now_ms: u64,
) -> OptionalObservation<String> {
    let joined = (!dmi.module_form_factors.is_empty()).then(|| dmi.module_form_factors.join(" / "));
    super::optional_from_source(joined, &dmi.status, now_ms)
}

/// Assemble the DMI module facts and zram/zswap compression facts into their
/// typed optional observations, honoring each field's exact failure receipt.
pub(super) fn assemble_module_and_compression_observations(
    dmi: &DmiMemoryObservation,
    compressed_swap: &CompressedSwapObservation,
    now_ms: u64,
) -> (MemoryModuleObservations, MemoryCompressionObservations) {
    let modules = MemoryModuleObservations {
        speed_mhz: optional_from_source_with_receipt(
            dmi.speed_mhz,
            &dmi.status,
            &dmi.receipts,
            DMI_SPEED_FIELD,
            now_ms,
        ),
        slots_used: optional_from_source_with_receipt(
            dmi.slots_used,
            &dmi.status,
            &dmi.receipts,
            DMI_SLOTS_USED_FIELD,
            now_ms,
        ),
        slots_total: optional_from_source_with_receipt(
            dmi.slots_total,
            &dmi.status,
            &dmi.receipts,
            DMI_SLOTS_TOTAL_FIELD,
            now_ms,
        ),
        module_type: module_type_observation(dmi, now_ms),
        manufacturer: module_manufacturer_observation(dmi, now_ms),
        form_factor: module_form_factor_observation(dmi, now_ms),
    };
    let compression = MemoryCompressionObservations {
        // Linux zram/zswap are represented separately below and must not
        // masquerade as resident compressed-memory accounting.
        compressed_memory_used_bytes: OptionalObservation::unavailable(FailureKind::Unsupported),
        compressed_swap_used_bytes: optional_from_source_with_receipt(
            compressed_swap.zram_swap_used_bytes,
            &compressed_swap.status,
            &compressed_swap.receipts,
            ZRAM_USED_FIELD,
            now_ms,
        ),
        compressed_swap_capacity_bytes: optional_from_source_with_receipt(
            compressed_swap.zram_total_bytes,
            &compressed_swap.status,
            &compressed_swap.receipts,
            ZRAM_TOTAL_FIELD,
            now_ms,
        ),
        compressed_swap_cache_enabled: optional_from_source_with_receipt(
            compressed_swap.zswap_enabled,
            &compressed_swap.status,
            &compressed_swap.receipts,
            ZSWAP_ENABLED_FIELD,
            now_ms,
        ),
        compressed_swap_original_bytes: optional_from_source_with_receipt(
            compressed_swap.zram_original_bytes,
            &compressed_swap.status,
            &compressed_swap.receipts,
            ZRAM_ORIGINAL_FIELD,
            now_ms,
        ),
        compressed_swap_compressed_bytes: optional_from_source_with_receipt(
            compressed_swap.zram_compressed_bytes,
            &compressed_swap.status,
            &compressed_swap.receipts,
            ZRAM_COMPRESSED_FIELD,
            now_ms,
        ),
        compressed_swap_memory_used_bytes: optional_from_source_with_receipt(
            compressed_swap.zram_memory_used_bytes,
            &compressed_swap.status,
            &compressed_swap.receipts,
            ZRAM_MEMORY_USED_FIELD,
            now_ms,
        ),
    };
    (modules, compression)
}

#[cfg(test)]
#[path = "../../../../tests/headless/engine/collector/compute/memory_sources.rs"]
mod tests;
