//! Linux CPU topology and instruction-feature inventory source.

use super::*;

#[derive(Debug, Default)]
pub(super) struct ComputeTopologySource;

impl InventorySource for ComputeTopologySource {
    type Value = ComputeTopology;

    fn collect(&mut self, context: &InventoryContext<'_>) -> SourceFragment<Self::Value> {
        let mut failures = FailureSummary::default();
        let logical_cpu_count_from_root = match count_logical_cpus(&context.paths.cpu_root) {
            Ok(count) => {
                if count == 0 {
                    failures.record(FailureKind::TemporarilyUnavailable);
                    None
                } else {
                    Some(count)
                }
            }
            Err(error) => {
                failures.record_io(&error);
                None
            }
        };
        let cpu_root_available = logical_cpu_count_from_root.is_some();
        let sysfs_base_frequency_mhz = read_sysfs_base_frequency_mhz(context, &mut failures);
        // Linux's cpufreq base_frequency is the kernel's policy-aware static
        // base-clock fact. It must win over CPUID leaf 0x16: on hybrid CPUs
        // that leaf can describe one core class or a nominal package value,
        // which is not the system base speed shown by the OS. CPUID remains a
        // bounded static fallback when no cpufreq policy exposes a base. A
        // live frequency sample never enters this static field.
        let cpuid_frequencies = context
            .paths
            .uses_native_cpu_root()
            .then(advertised_frequencies_mhz)
            .unwrap_or((None, None));
        let base_frequency_mhz = sysfs_base_frequency_mhz.or(cpuid_frequencies.0);
        let logical_cpu_count = context
            .system
            .logical_cpu_count
            .or(logical_cpu_count_from_root);
        let instruction_features = match fs::read_to_string(context.paths.proc_root.join("cpuinfo"))
        {
            Ok(text) => parse_cpuinfo_instruction_features(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                failures.record_io(&error);
                Vec::new()
            }
        };
        let cpu_identity = context
            .paths
            .uses_native_cpu_root()
            .then(probe_cpu_identity)
            .unwrap_or_default();
        let logical_cpu_count_value = logical_cpu_count.unwrap_or_default();
        let (core_breakdown, cpu_types, socket_count) =
            if cpu_root_available && context.paths.uses_native_cpu_root() {
                let (p_cores, e_cores, lp_cores) = detect_cpu_core_breakdown();
                (
                    CoreBreakdown {
                        p_cores,
                        e_cores,
                        lp_cores,
                    },
                    detect_cpu_types(logical_cpu_count_value),
                    detect_socket_count(),
                )
            } else {
                (
                    CoreBreakdown {
                        p_cores: 0,
                        e_cores: 0,
                        lp_cores: 0,
                    },
                    vec![CpuType::Unknown; logical_cpu_count_value],
                    None,
                )
            };
        let base_observed = [
            context.system.cpu_brand.is_some(),
            logical_cpu_count.is_some(),
            context.system.total_memory_mb.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        // A readable CPU topology root authoritatively supplies the breakdown,
        // logical CPU typing and package/socket view. Base frequency is
        // optional on CPUs without a cpufreq driver.
        let observed = base_observed + usize::from(cpu_root_available) * 3;
        let item_count = observed
            + usize::from(base_frequency_mhz.is_some())
            + usize::from(!instruction_features.is_empty())
            + usize::from(cpu_identity.is_present());

        SourceFragment::new(
            ComputeTopology {
                cpu_brand: context.system.cpu_brand.clone(),
                logical_cpu_count,
                socket_count,
                total_memory_mb: context.system.total_memory_mb,
                core_breakdown,
                cpu_types,
                base_frequency_mhz,
                instruction_features,
                cpu_identity,
            },
            TOPOLOGY_PROVIDER,
            required_source_outcome(observed, 6, &failures),
            item_count,
        )
    }
}

/// Read the highest policy base exposed by Linux's cpufreq sysfs ABI.
///
/// A heterogeneous processor can expose more than one policy, and the
/// product's single static "Base" field represents the highest advertised
/// processor class. The explicit fixture path is always checked first; native
/// policy discovery is only enabled for the real host root so synthetic tests
/// cannot leak the test runner's CPU facts.
fn read_sysfs_base_frequency_mhz(
    context: &InventoryContext<'_>,
    failures: &mut FailureSummary,
) -> Option<u64> {
    let mut candidates = Vec::new();
    match fs::read_to_string(&context.paths.base_frequency) {
        Ok(text) => {
            if let Some(mhz) = parse_frequency_khz_to_mhz(&text) {
                candidates.push(mhz);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => failures.record_io(&error),
    }

    if context.paths.uses_native_cpu_root() {
        let policy_root = context.paths.cpu_root.join("cpufreq");
        let entries = match fs::read_dir(policy_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return candidates.into_iter().max();
            }
            Err(error) => {
                failures.record_io(&error);
                return candidates.into_iter().max();
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !entry.file_name().to_string_lossy().starts_with("policy") {
                continue;
            }
            match fs::read_to_string(path.join("base_frequency")) {
                Ok(text) => {
                    if let Some(mhz) = parse_frequency_khz_to_mhz(&text) {
                        candidates.push(mhz);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => failures.record_io(&error),
            }
        }
    }

    candidates.into_iter().max()
}

fn parse_frequency_khz_to_mhz(raw: &str) -> Option<u64> {
    let khz = raw.trim().parse::<u64>().ok()?;
    let mhz = khz / 1000;
    (mhz > 0 && mhz < 10_000).then_some(mhz)
}

/// Read advertised processor frequencies from the safe CPUID wrapper. CPUID is
/// consulted only as a static fallback on the native host path; fixture roots
/// must not leak the test runner's CPU facts into a synthetic inventory.
fn advertised_frequencies_mhz() -> (Option<u64>, Option<u64>) {
    #[cfg(all(
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    ))]
    {
        let cpuid = raw_cpuid::CpuId::new();
        let frequency = cpuid.get_processor_frequency_info();
        let base = frequency
            .as_ref()
            .map(|info| u64::from(info.processor_base_frequency()))
            .filter(|frequency| *frequency > 0 && *frequency < 10_000);
        let max = frequency
            .as_ref()
            .map(|info| u64::from(info.processor_max_frequency()))
            .filter(|frequency| *frequency > 0 && *frequency < 10_000);
        (base, max)
    }
    #[cfg(not(all(
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    )))]
    {
        (None, None)
    }
}

/// Read the CPUID version identity (leaf-0 vendor string, leaf-1 `EAX` fields)
/// from the safe CPUID wrapper. Only the typed fields cross into the domain;
/// the `EAX` bit layout stays in this adapter. Fixture roots must not leak the
/// test runner's CPU facts into a synthetic inventory, so callers gate this on
/// the native host path exactly like [`advertised_frequencies_mhz`].
fn probe_cpu_identity() -> CpuIdentity {
    #[cfg(all(
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    ))]
    {
        let cpuid = raw_cpuid::CpuId::new();
        let vendor = cpuid
            .get_vendor_info()
            .map(|vendor| vendor.as_str().to_string());
        cpuid
            .get_feature_info()
            .map_or_else(CpuIdentity::default, |info| {
                CpuIdentity::from_cpuid_parts(
                    vendor,
                    info.base_family_id(),
                    info.extended_family_id(),
                    info.base_model_id(),
                    info.extended_model_id(),
                    info.stepping_id(),
                )
            })
    }
    #[cfg(not(all(
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    )))]
    {
        CpuIdentity::default()
    }
}

/// The native `/proc/cpuinfo` flag token for one neutral feature. This is the
/// single mapping table — unknown tokens stay unmapped and an unreported
/// feature is never guessed.
fn cpuinfo_native_flag(feature: CpuInstructionFeature) -> &'static str {
    match feature {
        CpuInstructionFeature::Sse41 => "sse4_1",
        CpuInstructionFeature::Sse42 => "sse4_2",
        CpuInstructionFeature::Avx => "avx",
        CpuInstructionFeature::Avx2 => "avx2",
        CpuInstructionFeature::Avx512F => "avx512f",
        CpuInstructionFeature::Fma3 => "fma",
        CpuInstructionFeature::AesNi => "aes",
        CpuInstructionFeature::ShaNi => "sha_ni",
        CpuInstructionFeature::AvxVnni => "avx_vnni",
        CpuInstructionFeature::Avx512Vnni => "avx512_vnni",
        CpuInstructionFeature::AmxInt8 => "amx_int8",
        CpuInstructionFeature::AmxBf16 => "amx_bf16",
        CpuInstructionFeature::Neon => "asimd",
        CpuInstructionFeature::Sve => "sve",
    }
}

/// Extract the instruction-feature list from `/proc/cpuinfo` text. Only the
/// first feature line is read (every processor block repeats it); results are
/// emitted in the neutral enum's canonical order so UI order is deterministic.
/// An empty result means no recognizable feature line — an honest absence.
pub(super) fn parse_cpuinfo_instruction_features(cpuinfo: &str) -> Vec<CpuInstructionFeature> {
    let Some(feature_line) = cpuinfo.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        let key = key.trim();
        (key == "flags" || key == "Features").then(|| value.trim())
    }) else {
        return Vec::new();
    };
    let tokens: std::collections::HashSet<&str> = feature_line.split_whitespace().collect();
    CpuInstructionFeature::ALL
        .iter()
        .copied()
        .filter(|feature| tokens.contains(cpuinfo_native_flag(*feature)))
        .collect()
}

fn count_logical_cpus(root: &Path) -> io::Result<usize> {
    Ok(fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix("cpu"))
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
                })
        })
        .count())
}
