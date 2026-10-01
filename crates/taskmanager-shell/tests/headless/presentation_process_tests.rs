use super::*;
use taskmanager_core::LimitValue;

#[test]
fn format_open_files_saturation_variants() {
    assert_eq!(
        format_open_files_saturation(
            42,
            Some(LimitValue::Value(1024)),
            Some(LimitValue::Value(4096)),
            "∞",
        ),
        Some("42 / 1024 (4%) [max 4096]".to_string())
    );
    assert_eq!(
        format_open_files_saturation(
            512,
            Some(LimitValue::Value(1024)),
            Some(LimitValue::Value(1024)),
            "∞",
        ),
        Some("512 / 1024 (50%)".to_string())
    );
    assert_eq!(
        format_open_files_saturation(
            10,
            Some(LimitValue::Value(100)),
            Some(LimitValue::Unlimited),
            "Unlimited",
        ),
        Some("10 / 100 (10%) [max Unlimited]".to_string())
    );
    assert_eq!(
        format_open_files_saturation(
            10,
            Some(LimitValue::Unlimited),
            Some(LimitValue::Unlimited),
            "∞",
        ),
        Some("10 / ∞".to_string())
    );
    assert_eq!(format_open_files_saturation(10, None, None, "∞"), None);
}
