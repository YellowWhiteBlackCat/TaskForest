use crate::FrontendShape;
use crate::presentation::parity::{PROBES, VerdictField, unexplained_differences, verdict};
use crate::presentation::thresholds::divergent_axes;
use crate::presentation::vocabulary::{
    LayoutProfile, PerformanceChartInventory, PerformanceVerticalRunway, VerticalSpace,
};

#[test]
fn the_probe_grid_is_boundary_bearing_not_a_constant_replay() {
    assert!(PROBES.len() >= 10);
    let mut profiles = std::collections::HashSet::new();
    let mut verticals = std::collections::HashSet::new();
    let mut runways = std::collections::HashSet::new();
    let mut inventories = std::collections::HashSet::new();
    for probe in PROBES {
        let shape = verdict(FrontendShape::Gpui, *probe);
        profiles.insert(format!("{:?}", shape.profile));
        verticals.insert(format!("{:?}", shape.vertical_space));
        runways.insert(format!("{:?}", shape.runway));
        inventories.insert(format!("{:?}", shape.chart_inventory));
    }
    assert!(
        profiles.len() >= 3,
        "the probes must exercise several horizontal buckets: {profiles:?}"
    );
    assert!(
        verticals.len() >= 2,
        "the probes must exercise several vertical buckets: {verticals:?}"
    );
    assert!(
        runways.len() >= 2,
        "the probes must exercise several rungs: {runways:?}"
    );
    assert!(
        inventories.len() == 2,
        "both chart-inventory product bits must be reachable: {inventories:?}"
    );
    assert_eq!(
        PROBES
            .iter()
            .map(|probe| probe.name)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        PROBES.len(),
        "every probe has a stable name for gate output"
    );
}

#[test]
fn the_reference_shape_classifies_the_reference_window_to_its_known_verdict() {
    let probe = PROBES
        .iter()
        .find(|probe| probe.name == "desktop-1280x720")
        .expect("reference window is in the grid");
    let shape = verdict(FrontendShape::Gpui, *probe);
    assert_eq!(shape.profile, LayoutProfile::Standard);
    assert_eq!(shape.vertical_space, VerticalSpace::Standard);
    assert_eq!(shape.runway, PerformanceVerticalRunway::Charts);
    assert_eq!(shape.chart_inventory, PerformanceChartInventory::Full);
}

#[test]
fn every_shape_pair_agrees_or_the_difference_is_registered() {
    for (index, left) in FrontendShape::ALL.iter().enumerate() {
        for right in FrontendShape::ALL.iter().skip(index + 1) {
            let differences = unexplained_differences(*left, *right);
            assert!(
                differences.is_empty(),
                "{} x {}: unexplained layout verdict differences: {differences:?}",
                left.name(),
                right.name()
            );
        }
    }
}

#[test]
fn pure_folds_are_shared_by_every_shape_on_every_probe() {
    for probe in PROBES {
        let reference = verdict(FrontendShape::Gpui, *probe);
        for shape in FrontendShape::ALL {
            let candidate = verdict(shape, *probe);
            assert_eq!(
                candidate.profile,
                reference.profile,
                "{}:{} profile is a pure shared fold",
                shape.name(),
                probe.name
            );
            assert_eq!(
                candidate.chart_inventory,
                reference.chart_inventory,
                "{}:{} chart inventory is a pure fold of the shared width axis and the \
                 registered runway",
                shape.name(),
                probe.name
            );
        }
    }
}

#[test]
fn the_registered_axes_cover_every_field_that_may_differ() {
    let registered = divergent_axes(FrontendShape::Gpui, FrontendShape::Iced);
    for field in VerdictField::ALL {
        if field.axes_governing().is_empty() {
            assert!(
                !registered.is_empty() == false || field == VerdictField::Profile,
                "a pure fold must not be smuggled into the registered-explanation set"
            );
        }
    }
}
