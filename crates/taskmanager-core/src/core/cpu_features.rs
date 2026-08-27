//! Platform-neutral CPU instruction-set capabilities.
//!
//! Native adapters map their own vocabulary (x86 CPUID leaves, Linux
//! `/proc/cpuinfo` flags, macOS `sysctl hw.optional.*`) into this closed enum
//! so frontends render one stable list without platform conditionals. A
//! feature the native source did not report is absent from the list; absence
//! is never replaced with a guessed value.

use serde::{Deserialize, Serialize};

/// One instruction-set capability reported by the native CPU source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CpuInstructionFeature {
    Sse41,
    Sse42,
    Avx,
    Avx2,
    Avx512F,
    Fma3,
    AesNi,
    ShaNi,
    /// AVX-VNNI (VEX-encoded neural-network instructions, e.g. Lunar Lake /
    /// Meteor Lake P-cores).
    AvxVnni,
    /// AVX-512-VNNI (EVEX-encoded neural-network instructions).
    Avx512Vnni,
    /// Intel Advanced Matrix Extensions, INT8 tiles.
    AmxInt8,
    /// Intel Advanced Matrix Extensions, BF16 tiles.
    AmxBf16,
    /// ARM Advanced SIMD.
    Neon,
    /// ARM Scalable Vector Extension.
    Sve,
}

impl CpuInstructionFeature {
    /// Complete variant list. Tests and consumers enumerate this instead of
    /// maintaining a duplicated list.
    pub const ALL: &'static [Self] = &[
        Self::Sse41,
        Self::Sse42,
        Self::Avx,
        Self::Avx2,
        Self::Avx512F,
        Self::Fma3,
        Self::AesNi,
        Self::ShaNi,
        Self::AvxVnni,
        Self::Avx512Vnni,
        Self::AmxInt8,
        Self::AmxBf16,
        Self::Neon,
        Self::Sve,
    ];

    /// Display label for hardware detail panels.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sse41 => "SSE4.1",
            Self::Sse42 => "SSE4.2",
            Self::Avx => "AVX",
            Self::Avx2 => "AVX2",
            Self::Avx512F => "AVX-512F",
            Self::Fma3 => "FMA3",
            Self::AesNi => "AES-NI",
            Self::ShaNi => "SHA-NI",
            Self::AvxVnni => "AVX-VNNI",
            Self::Avx512Vnni => "AVX-512 VNNI",
            Self::AmxInt8 => "AMX-INT8",
            Self::AmxBf16 => "AMX-BF16",
            Self::Neon => "NEON",
            Self::Sve => "SVE",
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_cpu_features_tests.rs"]
mod tests;
