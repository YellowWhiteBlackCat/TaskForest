use super::*;

#[test]
fn failures_display_without_panicking() {
    for failure in [
        InstanceFailure::Unsupported,
        InstanceFailure::MissingDependency,
        InstanceFailure::Rejected,
    ] {
        assert!(!failure.to_string().is_empty());
    }
}
