use super::*;

#[test]
fn startup_task_enumeration_is_typed_on_every_host() {
    let result = enumerate_startup_tasks();
    #[cfg(windows)]
    {
        match result {
            Ok(tasks) => {
                for task in &tasks {
                    assert!(
                        task.task_path.starts_with('\\'),
                        "task paths are root-relative backslash paths: {:?}",
                        task.task_path
                    );
                    assert!(
                        task.has_logon_or_boot_trigger,
                        "only logon/boot-triggered tasks are startup items"
                    );
                }
            }
            // Honest degradation on hosts where the service is stopped or
            // locked down, or the store exceeds the bounded enumeration.
            Err(
                WindowsApiError::PermissionDenied
                | WindowsApiError::QueryFailed
                | WindowsApiError::ResourceLimit,
            ) => {}
            Err(other) => panic!("unexpected task scheduler failure: {other:?}"),
        }
    }
    #[cfg(not(windows))]
    {
        assert_eq!(result, Err(WindowsApiError::Unsupported));
    }
}

#[cfg(windows)]
#[test]
fn collection_index_variant_carries_the_documented_vt_i4_discriminant() {
    use windows::Win32::System::Variant::VT_I4;

    let variant = index_variant(7);
    assert_eq!(variant.vt(), VT_I4);
    assert_eq!(
        i32::try_from(&variant).expect("a VT_I4 variant decodes as i32"),
        7
    );
}
