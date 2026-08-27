use super::zero_value_color;
use taskmanager_theme::Color;

#[test]
fn zero_value_policy_dims_only_enabled_zero_metrics() {
    let metric = Color::from_hex(0x3399cc);
    let muted = Color::from_hex(0x777777);

    assert_eq!(zero_value_color(metric, muted, true, true), muted);
    assert_eq!(zero_value_color(metric, muted, true, false), metric);
    assert_eq!(zero_value_color(metric, muted, false, true), metric);
}
