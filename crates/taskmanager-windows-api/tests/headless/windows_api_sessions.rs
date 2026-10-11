use super::*;

#[cfg(windows)]
#[test]
fn native_session_enumeration_is_bounded_and_not_unsupported() {
    let result = enumerate_sessions();
    assert!(!matches!(result, Err(WindowsApiError::Unsupported)));
    if let Ok(sessions) = result {
        assert!(sessions.len() <= MAX_WTS_SESSIONS as usize);
    }
}

#[test]
fn wts_infoex_requires_exact_bounded_response_size() {
    assert_eq!(validate_wts_infoex_size(256, 256), Ok(()));
    assert_eq!(
        validate_wts_infoex_size(255, 256),
        Err(WindowsApiError::QueryFailed)
    );
    assert_eq!(
        validate_wts_infoex_size(1025, 256),
        Err(WindowsApiError::ResourceLimit)
    );
    assert_eq!(
        validate_wts_infoex_size(0, 0),
        Err(WindowsApiError::QueryFailed)
    );
}

#[test]
fn wts_incarnation_requires_level_one_matching_session_and_positive_logon_time() {
    let expected = WindowsSessionIncarnation::new(12_345).expect("nonzero fixture");
    assert_eq!(parse_wts_session_incarnation(7, 1, 7, 12_345), Ok(expected));
    assert_eq!(
        parse_wts_session_incarnation(7, 0, 7, 12_345),
        Err(WindowsApiError::Unsupported)
    );
    assert_eq!(
        parse_wts_session_incarnation(7, 1, 8, 12_345),
        Err(WindowsApiError::IdentityChanged)
    );
    assert_eq!(
        parse_wts_session_incarnation(7, 1, 7, 0),
        Err(WindowsApiError::IdentityChanged)
    );
    assert_eq!(
        parse_wts_session_incarnation(7, 1, 7, -1),
        Err(WindowsApiError::IdentityChanged)
    );
}

#[test]
fn native_identity_cannot_be_constructed_for_session_zero_or_zero_logon_time() {
    assert_eq!(WindowsSessionIncarnation::new(0), None);
    let incarnation = WindowsSessionIncarnation::new(1).expect("nonzero fixture");
    assert_eq!(WindowsSessionIdentity::new(0, incarnation), None);
    assert!(WindowsSessionIdentity::new(1, incarnation).is_some());
}

#[test]
fn logoff_dispatch_requeries_exact_incarnation_before_native_call() {
    use std::cell::Cell;

    let incarnation = WindowsSessionIncarnation::new(21).expect("nonzero fixture");
    let target = WindowsSessionIdentity::new(7, incarnation).expect("valid target");
    let query_count = Cell::new(0);
    let dispatch_count = Cell::new(0);
    let result = dispatch_logoff_after_revalidation(
        target,
        |session_id| {
            assert_eq!(session_id, 7);
            query_count.set(query_count.get() + 1);
            Ok(incarnation)
        },
        |session_id| {
            assert_eq!(session_id, 7);
            dispatch_count.set(dispatch_count.get() + 1);
            Ok(())
        },
    );

    assert_eq!(result, Ok(()));
    assert_eq!(query_count.get(), 1);
    assert_eq!(dispatch_count.get(), 1);
}

#[test]
fn logoff_dispatch_rejects_reused_session_id_without_native_call() {
    use std::cell::Cell;

    let target_incarnation = WindowsSessionIncarnation::new(21).expect("nonzero fixture");
    let replacement_incarnation = WindowsSessionIncarnation::new(22).expect("nonzero fixture");
    let target = WindowsSessionIdentity::new(7, target_incarnation).expect("valid target");
    let dispatch_count = Cell::new(0);
    let result = dispatch_logoff_after_revalidation(
        target,
        |_| Ok(replacement_incarnation),
        |_| {
            dispatch_count.set(dispatch_count.get() + 1);
            Ok(())
        },
    );

    assert_eq!(result, Err(WindowsApiError::IdentityChanged));
    assert_eq!(dispatch_count.get(), 0);
}

#[test]
fn logoff_dispatch_query_failure_never_calls_native_api() {
    use std::cell::Cell;

    let incarnation = WindowsSessionIncarnation::new(21).expect("nonzero fixture");
    let target = WindowsSessionIdentity::new(7, incarnation).expect("valid target");
    let dispatch_count = Cell::new(0);
    let result = dispatch_logoff_after_revalidation(
        target,
        |_| Err(WindowsApiError::PermissionDenied),
        |_| {
            dispatch_count.set(dispatch_count.get() + 1);
            Ok(())
        },
    );

    assert_eq!(result, Err(WindowsApiError::PermissionDenied));
    assert_eq!(dispatch_count.get(), 0);
}

#[test]
fn workstation_lock_is_limited_to_exact_current_session_identity() {
    use std::cell::Cell;

    let selected_incarnation = WindowsSessionIncarnation::new(21).expect("nonzero fixture");
    let selected = WindowsSessionIdentity::new(7, selected_incarnation).expect("valid target");
    let current = WindowsSessionIdentity::new(7, selected_incarnation).expect("valid current");
    let dispatch_count = Cell::new(0);
    let result = dispatch_lock_only_for_current_session(
        selected,
        || Ok(current),
        || {
            dispatch_count.set(dispatch_count.get() + 1);
            Ok(())
        },
    );
    assert_eq!(result, Ok(()));
    assert_eq!(dispatch_count.get(), 1);
}

#[test]
fn workstation_lock_rejects_other_session_and_reused_current_session() {
    use std::cell::Cell;

    let selected_incarnation = WindowsSessionIncarnation::new(21).expect("nonzero fixture");
    let selected = WindowsSessionIdentity::new(7, selected_incarnation).expect("valid target");
    let dispatch_count = Cell::new(0);
    let other_session = WindowsSessionIdentity::new(
        8,
        WindowsSessionIncarnation::new(21).expect("nonzero fixture"),
    )
    .expect("valid current");
    let result = dispatch_lock_only_for_current_session(
        selected,
        || Ok(other_session),
        || {
            dispatch_count.set(dispatch_count.get() + 1);
            Ok(())
        },
    );
    assert_eq!(result, Err(WindowsApiError::Unsupported));

    let replaced_current = WindowsSessionIdentity::new(
        7,
        WindowsSessionIncarnation::new(22).expect("nonzero fixture"),
    )
    .expect("valid replacement");
    let result = dispatch_lock_only_for_current_session(
        selected,
        || Ok(replaced_current),
        || {
            dispatch_count.set(dispatch_count.get() + 1);
            Ok(())
        },
    );
    assert_eq!(result, Err(WindowsApiError::IdentityChanged));
    assert_eq!(dispatch_count.get(), 0);
}

#[test]
fn workstation_lock_query_failure_never_calls_native_api() {
    use std::cell::Cell;

    let incarnation = WindowsSessionIncarnation::new(21).expect("nonzero fixture");
    let target = WindowsSessionIdentity::new(7, incarnation).expect("valid target");
    let dispatch_count = Cell::new(0);
    let result = dispatch_lock_only_for_current_session(
        target,
        || Err(WindowsApiError::PermissionDenied),
        || {
            dispatch_count.set(dispatch_count.get() + 1);
            Ok(())
        },
    );

    assert_eq!(result, Err(WindowsApiError::PermissionDenied));
    assert_eq!(dispatch_count.get(), 0);
}
