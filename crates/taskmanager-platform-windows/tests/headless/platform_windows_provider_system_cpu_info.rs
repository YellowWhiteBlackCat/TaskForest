use super::*;

#[test]
fn zero_cpuid_frequency_is_unavailable() {
    assert_eq!(nonzero_u16(0), None);
    assert_eq!(nonzero_u16(2_400), Some(2_400));
}

#[test]
fn zero_filled_cpuid_leaf_falls_back_to_power_base_and_smbios_turbo() {
    assert_eq!(
        resolve_frequency_sources(None, None, Some(1900), Some(4800)),
        (Some(1900), Some(4800))
    );
    assert_eq!(
        resolve_frequency_sources(Some(2400), Some(5200), Some(1900), Some(4800)),
        (Some(1900), Some(5200)),
        "policy/core-type-aware power information wins for the base"
    );
}

#[test]
fn unresolved_frequency_sources_stay_absent() {
    assert_eq!(
        resolve_frequency_sources(None, None, None, None),
        (None, None)
    );
    assert_eq!(
        resolve_frequency_sources(Some(0), Some(0), None, None),
        (None, None)
    );
}

#[test]
fn detected_features_stay_within_the_neutral_vocabulary() {
    let features = detected_instruction_features();
    for feature in features {
        assert!(
            CpuInstructionFeature::ALL.contains(&feature),
            "unexpected feature {} outside ALL",
            feature.label()
        );
    }
}

#[test]
fn detected_features_are_emitted_in_canonical_order() {
    let features = detected_instruction_features();
    let canonical: Vec<CpuInstructionFeature> = CpuInstructionFeature::ALL.to_vec();
    let mut positions = features.iter().map(|feature| {
        canonical
            .iter()
            .position(|candidate| candidate == feature)
            .map(|index| index as u64)
    });
    let mut last: Option<u64> = None;
    for position in positions.by_ref() {
        let Some(position) = position else {
            panic!("feature outside canonical vocabulary");
        };
        if let Some(previous) = last {
            assert!(position > previous, "features must be strictly ascending");
        }
        last = Some(position);
    }
}
