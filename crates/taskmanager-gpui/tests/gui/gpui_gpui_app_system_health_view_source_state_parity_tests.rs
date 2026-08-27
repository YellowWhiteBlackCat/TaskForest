use super::{DeviceStatus, SourceStateKind, source_state_color, state_color};
use crate::gpui_app::theme::Theme;

/// Parity: the badge's render input is exactly the neutral VM's kind. The
/// reroute through `SourceStateKind` must reproduce the historical tone
/// for every device status — healthy=cpu, stale/missing-tool=disk,
/// denied=danger, unsupported=dim.
#[test]
fn badge_colors_follow_the_neutral_kind_without_changing_tone() {
    let theme = Theme::dark();
    let cases = [
        (DeviceStatus::Healthy, theme.cpu, SourceStateKind::Ok),
        (DeviceStatus::Stale, theme.disk, SourceStateKind::Stale),
        (
            DeviceStatus::MissingTool,
            theme.disk,
            SourceStateKind::Degraded,
        ),
        (
            DeviceStatus::PermissionDenied,
            theme.danger,
            SourceStateKind::Failed,
        ),
        (
            DeviceStatus::Unsupported,
            theme.fg_dim,
            SourceStateKind::Unknown,
        ),
    ];
    for (status, expected, kind) in cases {
        assert_eq!(SourceStateKind::from_device_status(status), kind);
        assert_eq!(state_color(&theme, status), expected, "tone for {status:?}");
        assert_eq!(
            source_state_color(&theme, kind),
            expected,
            "kind map for {kind:?}"
        );
    }
}
