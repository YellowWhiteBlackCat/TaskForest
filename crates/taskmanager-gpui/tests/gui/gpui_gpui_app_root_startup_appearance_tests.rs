use super::fallback;
use taskmanager_core::core::appearance::DesktopFamily;

#[test]
fn timeout_fallback_keeps_target_platform_identity() {
    let observation = fallback(Vec::new());
    let expected = if cfg!(target_os = "windows") {
        DesktopFamily::Windows
    } else {
        DesktopFamily::Unknown
    };
    assert_eq!(observation.value.family, expected);
    assert!(observation.value.color_scheme == Default::default());
    assert!(observation.value.high_contrast.is_none());
}
