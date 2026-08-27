use super::*;

#[test]
fn service_target_parsing_round_trips() {
    assert_eq!(
        parse_service_target(&ServiceId::new("macos:gui:501:com.apple.test")),
        ("gui/501".to_string(), "com.apple.test".to_string())
    );
    assert_eq!(
        parse_service_target(&ServiceId::new("macos:system:com.apple.bsd")),
        ("system".to_string(), "com.apple.bsd".to_string())
    );
}

#[test]
fn log_line_levels_are_stable() {
    assert_eq!(
        log_line_level("2026-08-02 10:00:00.123456+0800  com.apple.test[123:456] info: hello"),
        ServiceLogLevel::Info
    );
    assert_eq!(
        log_line_level("2026-08-02 10:00:00.123456+0800  com.apple.test[123:456] error: boom"),
        ServiceLogLevel::Error
    );
    assert_eq!(
        log_line_level("2026-08-02 10:00:00.123456+0800  com.apple.test[123:456] warning: w"),
        ServiceLogLevel::Warning
    );
    assert_eq!(log_line_level("not a log line"), ServiceLogLevel::Unknown);
}
