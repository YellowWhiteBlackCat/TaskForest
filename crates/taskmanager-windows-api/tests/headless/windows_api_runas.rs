//! Headless contract tests for the audited `runas` call group. The pure
//! HRESULT→Win32 extraction runs on EVERY host (it is the classification
//! input for the launch-failure table); the native calls themselves are
//! Windows-only and are receipt territory, never headless fabrication.

use super::*;

#[test]
fn hresult_win32_extraction_recovers_the_classified_launch_codes() {
    // HRESULT_FROM_WIN32(2) and HRESULT_FROM_WIN32(1223) must round-trip to
    // their Win32 codes: those two drive the missing-install and user-refusal
    // transport facts.
    assert_eq!(win32_code_from_hresult(0x8007_0002_u32 as i32), 2);
    assert_eq!(win32_code_from_hresult(0x8007_04C7_u32 as i32), 1223);
    assert_eq!(win32_code_from_hresult(0x8007_0057_u32 as i32), 87);
}

#[test]
fn hresult_win32_extraction_keeps_unattributable_failures_neutral() {
    // A plain COM failure (E_FAIL) or an NTSTATUS-shaped code keeps its raw
    // bits: it can never collide with 2 or 1223, so the transport-fact table
    // classifies it as the neutral AuthorizationUnavailable instead of
    // inventing a missing install or a user refusal.
    assert_eq!(win32_code_from_hresult(0x8000_4005_u32 as i32), 0x8000_4005);
    assert_eq!(win32_code_from_hresult(0xC000_0022_u32 as i32), 0xC000_0022);
    assert_ne!(win32_code_from_hresult(-2147467259), 2);
    assert_ne!(win32_code_from_hresult(-2147467259), 1223);
}

#[test]
fn non_windows_call_group_arms_fail_typed_without_touching_anything() {
    // Off Windows the boundary has no native arm: the session query is a
    // typed Unsupported and the launch reports the dormant call group —
    // nothing is spawned and no code is invented.
    #[cfg(not(windows))]
    {
        assert!(matches!(
            interactive_session_available(),
            Err(WindowsApiError::Unsupported)
        ));
        assert_eq!(
            run_elevated_and_wait("helper.exe", "1 2 kill", Duration::from_secs(1)),
            RunasLaunchOutcome::Unsupported
        );
    }
}
