//! Static CPU facts whose public APIs are safe and bounded.

use taskmanager_core::CpuInstructionFeature;

/// Read the advertised base and maximum processor frequencies from CPUID's
/// frequency-information leaf. Unsupported architectures, virtual CPUs, and
/// processors that omit the leaf remain unavailable rather than inferred from
/// a live clock sample.
pub(super) fn advertised_frequencies_mhz() -> (Option<u64>, Option<u64>) {
    #[cfg(all(
        windows,
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    ))]
    {
        let cpuid = raw_cpuid::CpuId::new();
        let frequency = cpuid.get_processor_frequency_info();
        let cpuid_base = frequency
            .as_ref()
            .and_then(|f| nonzero_u16(f.processor_base_frequency()));
        let cpuid_max = frequency
            .as_ref()
            .and_then(|f| nonzero_u16(f.processor_max_frequency()));

        // `PROCESSOR_POWER_INFORMATION.MaxMhz` reports the per-core-type base
        // frequency (P 1900 / E 1500 / LP-E 1500 on hybrid parts), never the
        // turbo ceiling — using it as the "max" row is the old bug.
        let power_base_max = taskmanager_windows_api::query_processor_power_information()
            .ok()
            .and_then(|powers| {
                powers
                    .iter()
                    .map(|p| p.max_mhz as u64)
                    .max()
                    .filter(|&m| m > 0 && m < 10_000)
            });

        // SMBIOS Type 4 `Max Speed` is the reliable non-privileged turbo source
        // on hybrid parts where CPUID leaf 0x16 is zero-filled (Panther Lake).
        let smbios_max = taskmanager_windows_api::query_smbios_processor_max_mhz()
            .filter(|&m| m > 0 && m < 10_000);

        resolve_frequency_sources(cpuid_base, cpuid_max, power_base_max, smbios_max)
    }
    #[cfg(not(all(
        windows,
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    )))]
    {
        (None, None)
    }
}

/// Read detected CPU instruction-set features from CPUID, mapped onto the
/// platform-neutral enum in its canonical order. A leaf the processor omits
/// contributes nothing; unsupported architectures return an honest empty list.
pub(super) fn detected_instruction_features() -> Vec<CpuInstructionFeature> {
    #[cfg(all(
        windows,
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    ))]
    {
        let cpuid = raw_cpuid::CpuId::new();
        let feature_info = cpuid.get_feature_info().map(|fi| {
            (
                fi.has_sse41(),
                fi.has_sse42(),
                fi.has_avx(),
                fi.has_fma(),
                fi.has_aesni(),
            )
        });
        let extended = cpuid.get_extended_feature_info().map(|efi| {
            (
                efi.has_avx2(),
                efi.has_avx512f(),
                efi.has_sha(),
                efi.has_avx_vnni(),
                efi.has_avx512vnni(),
                efi.has_amx_int8(),
                efi.has_amx_bf16(),
            )
        });
        CpuInstructionFeature::ALL
            .iter()
            .copied()
            .filter(|feature| match feature {
                CpuInstructionFeature::Sse41 => feature_info.is_some_and(|f| f.0),
                CpuInstructionFeature::Sse42 => feature_info.is_some_and(|f| f.1),
                CpuInstructionFeature::Avx => feature_info.is_some_and(|f| f.2),
                CpuInstructionFeature::Fma3 => feature_info.is_some_and(|f| f.3),
                CpuInstructionFeature::AesNi => feature_info.is_some_and(|f| f.4),
                CpuInstructionFeature::Avx2 => extended.is_some_and(|e| e.0),
                CpuInstructionFeature::Avx512F => extended.is_some_and(|e| e.1),
                CpuInstructionFeature::ShaNi => extended.is_some_and(|e| e.2),
                CpuInstructionFeature::AvxVnni => extended.is_some_and(|e| e.3),
                CpuInstructionFeature::Avx512Vnni => extended.is_some_and(|e| e.4),
                CpuInstructionFeature::AmxInt8 => extended.is_some_and(|e| e.5),
                CpuInstructionFeature::AmxBf16 => extended.is_some_and(|e| e.6),
                CpuInstructionFeature::Neon | CpuInstructionFeature::Sve => false,
            })
            .collect()
    }
    #[cfg(not(all(
        windows,
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    )))]
    {
        Vec::new()
    }
}

// Consumed only by the CPUID-backed Windows/x86 arm above and by the mounted
// headless frequency-source tests.
#[cfg(any(
    test,
    all(
        windows,
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    )
))]
fn nonzero_u16(value: u16) -> Option<u64> {
    (value > 0).then_some(u64::from(value))
}

/// Resolve the advertised base/max pair from the available static sources.
/// Windows' processor-power record is policy/core-type aware and therefore
/// wins for the base on hybrid parts; CPUID leaf 0x16 is a static fallback.
/// The turbo ceiling still prefers CPUID and falls back to SMBIOS when the
/// leaf is absent or zero-filled. A source that cannot be confirmed stays
/// `None` — never a live sample.
#[cfg(any(
    test,
    all(
        windows,
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    )
))]
fn resolve_frequency_sources(
    cpuid_base: Option<u64>,
    cpuid_max: Option<u64>,
    power_base_max: Option<u64>,
    smbios_max: Option<u64>,
) -> (Option<u64>, Option<u64>) {
    let nonzero = |value: Option<u64>| value.filter(|&m| m > 0);
    (
        nonzero(power_base_max).or(nonzero(cpuid_base)),
        nonzero(cpuid_max).or(nonzero(smbios_max)),
    )
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_system_cpu_info.rs"]
mod tests;
