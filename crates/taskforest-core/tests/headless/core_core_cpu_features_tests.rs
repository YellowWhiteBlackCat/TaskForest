use super::CpuInstructionFeature;
use std::collections::HashSet;

#[test]
fn all_lists_every_variant_exactly_once() {
    let mut seen = HashSet::new();
    for feature in CpuInstructionFeature::ALL {
        assert!(
            seen.insert(feature.label()),
            "duplicate label {} in ALL",
            feature.label()
        );
    }
    let serialized: Vec<String> = CpuInstructionFeature::ALL
        .iter()
        .map(|feature| serde_json::to_string(feature).expect("feature serializes"))
        .collect();
    let unique: HashSet<&String> = serialized.iter().collect();
    assert_eq!(
        unique.len(),
        CpuInstructionFeature::ALL.len(),
        "every variant must have a distinct wire form"
    );
}

#[test]
fn labels_round_trip_through_serde_by_variant() {
    for feature in CpuInstructionFeature::ALL {
        let json = serde_json::to_string(feature).expect("feature serializes");
        let decoded: CpuInstructionFeature = serde_json::from_str(&json).expect("feature decodes");
        assert_eq!(decoded, *feature);
        assert_eq!(decoded.label(), feature.label());
    }
}
