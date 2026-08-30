use super::*;

/// Table spot-checks against the transcribed libcpuid rows: exact display
/// pairs that were independently verified (upstream recognition tables plus
/// public CPUID references) must resolve; uncovered pairs stay `None`.
#[test]
fn intel_display_pairs_resolve_to_verified_codenames() {
    // Bloomfield i7-920: family 6, model 0x1A.
    assert_eq!(
        classify_cpu_codename(CpuVendor::Intel, 6, 0x1A),
        Some(("Nehalem (Bloomfield/Gainestown)", "45 nm"))
    );
    // Raptor Lake-S 13900K/14900K: family 6, model 183 (0xB7).
    let rpl = classify_cpu_codename(CpuVendor::Intel, 6, 183).unwrap();
    assert!(rpl.0.starts_with("Raptor Lake-S"));
    assert_eq!(rpl.1, "Intel 7");
    // 186 is Raptor Lake mobile; 191 is Raptor Lake-S low-tier — both were
    // commonly mislabeled as Arrow Lake and are deliberately pinned here.
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 186)
            .unwrap()
            .0
            .contains("Raptor Lake-P")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 191)
            .unwrap()
            .0
            .contains("Raptor Lake-S")
    );
    // Arrow Lake trio + Lunar Lake.
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 198)
            .unwrap()
            .0
            .contains("Arrow Lake-S")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 197)
            .unwrap()
            .0
            .contains("Arrow Lake-H")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 181)
            .unwrap()
            .0
            .contains("Arrow Lake-U")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 189)
            .unwrap()
            .0
            .contains("Lunar Lake")
    );
    // Xeon Scalable generations.
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 0x55)
            .unwrap()
            .0
            .contains("Skylake-SP")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 0x6A)
            .unwrap()
            .0
            .contains("Ice Lake-SP")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 0x8F)
            .unwrap()
            .0
            .contains("Sapphire")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 0xCF)
            .unwrap()
            .0
            .contains("Emerald")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Intel, 6, 0xAD)
            .unwrap()
            .0
            .contains("Granite")
    );
    // An uncovered model stays absent.
    assert_eq!(classify_cpu_codename(CpuVendor::Intel, 6, 0xEE), None);
}

/// AMD spot-checks: the Zen family split (0x17/0x19/0x1A) and the
/// frequently-confused Raphael/Cezanne model numbers.
#[test]
fn amd_display_pairs_resolve_to_verified_codenames() {
    // Vermeer 5950X: family 0x19, model 33 (0x21).
    assert_eq!(
        classify_cpu_codename(CpuVendor::Amd, 25, 33),
        Some(("Zen 3 (Vermeer)", "TSMC N7"))
    );
    // Raphael 7950X is model 97 (0x61) — NOT 80, which is Cezanne.
    assert!(
        classify_cpu_codename(CpuVendor::Amd, 0x19, 0x61)
            .unwrap()
            .0
            .contains("Raphael")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Amd, 0x19, 0x50)
            .unwrap()
            .0
            .contains("Cezanne")
    );
    // Summit Ridge 1800X / Matisse 3900X / Granite Ridge 9950X.
    assert!(
        classify_cpu_codename(CpuVendor::Amd, 0x17, 0x01)
            .unwrap()
            .0
            .contains("Summit Ridge")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Amd, 0x17, 0x71)
            .unwrap()
            .0
            .contains("Matisse")
    );
    assert!(
        classify_cpu_codename(CpuVendor::Amd, 0x1A, 0x44)
            .unwrap()
            .0
            .contains("Granite Ridge")
    );
    // Unknown pairs stay absent.
    assert_eq!(classify_cpu_codename(CpuVendor::Amd, 0x19, 0x7A), None);
    assert_eq!(classify_cpu_codename(CpuVendor::Amd, 0x1F, 0x01), None);
}

/// The vendor gate maps only the two native leaf-0 strings the table covers.
#[test]
fn vendor_gate_accepts_only_covered_vendor_strings() {
    assert_eq!(
        CpuVendor::from_vendor_id("GenuineIntel"),
        Some(CpuVendor::Intel)
    );
    assert_eq!(
        CpuVendor::from_vendor_id("AuthenticAMD"),
        Some(CpuVendor::Amd)
    );
    assert_eq!(CpuVendor::from_vendor_id("HygonGenuine"), None);
    assert_eq!(CpuVendor::from_vendor_id(""), None);
}
