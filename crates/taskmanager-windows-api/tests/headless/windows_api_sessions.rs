use super::*;

#[test]
fn native_session_enumeration_is_bounded_and_not_unsupported() {
    let result = enumerate_sessions();
    assert!(!matches!(result, Err(WindowsApiError::Unsupported)));
    if let Ok(sessions) = result {
        assert!(sessions.len() <= MAX_WTS_SESSIONS as usize);
    }
}

#[test]
fn session_zero_is_rejected_before_native_control() {
    assert_eq!(logoff_session(0), Err(WindowsApiError::InvalidInput));
}
