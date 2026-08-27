use super::*;

#[test]
fn classify_open_error_maps_eacces_and_eperm_to_permission_denied() {
    let eacces = io::Error::from_raw_os_error(ERR_EACCES);
    assert!(matches!(
        classify_open_error(eacces),
        SampleError::PermissionDenied(_)
    ));
    let eperm = io::Error::from_raw_os_error(ERR_EPERM);
    assert!(matches!(
        classify_open_error(eperm),
        SampleError::PermissionDenied(_)
    ));
}

#[test]
fn classify_open_error_maps_other_errno_to_open_failed() {
    let einval = io::Error::from_raw_os_error(22); // EINVAL
    assert!(matches!(
        classify_open_error(einval),
        SampleError::OpenFailed(_)
    ));
    // A non-OS error (no errno) is also OpenFailed, not PermissionDenied.
    let other = io::Error::other("synthetic");
    assert!(matches!(
        classify_open_error(other),
        SampleError::OpenFailed(_)
    ));
}

#[test]
fn finalize_returns_read_failed_when_no_engine_sampled() {
    let result = finalize(Vec::new(), Some(io::Error::other("read broke")));
    assert!(matches!(result, Err(SampleError::ReadFailed(_))));
    let empty = finalize(Vec::new(), None);
    assert!(matches!(empty, Err(SampleError::ReadFailed(_))));
}

#[test]
fn finalize_emits_engine_json_when_data_present() {
    let sampled = vec![SampledEngine {
        label: "Render/3D".to_string(),
        class_name: "render".to_string(),
        busy_pct: 37.5,
    }];
    let engines = finalize(sampled, None).expect("data present");
    assert_eq!(engines.len(), 1);
    assert_eq!(engines[0].name, "Render/3D");
    assert_eq!(engines[0].class, "render");
    assert!((engines[0].busy_pct - 37.5).abs() < 1e-6);
}

/// The xe/i915 sample fns OPEN real perf counters; on a CI host with no
/// Intel GPU PMU every open fails. Asserting the FAILURE PATH here exercises
/// the empty-pairs branch honestly — the dominant open error is surfaced
/// (as OpenFailed on a host with no PMU, since there is no EACCES to map to
/// PermissionDenied) and no fabricated engine is emitted.
#[test]
fn sample_with_no_real_pmu_surfaces_open_failure_honestly() {
    // An unregistered PMU type + a throwaway config: open_enabled fails.
    let layout = PmuLayout::Xe {
        pmu_type: u32::MAX,
        cpu: 0,
        engines: vec![XeEngineCfg {
            label: "Render/3D".to_string(),
            class: 0,
            class_name: "render".to_string(),
            instance: 0,
            active_config: 0x2,
            total_config: 0x3,
        }],
    };
    let result = sample(layout, 1);
    assert!(
        matches!(
            result,
            Err(SampleError::OpenFailed(_)) | Err(SampleError::PermissionDenied(_))
        ),
        "no real PMU in CI → typed open failure, got {result:?}"
    );
}

/// An empty engine list must answer a typed error — the caller-bug seam
/// the discovery guard normally prevents from being reached. Regression:
/// this used to panic on `first_open_error.expect("no pair opened")`.
#[test]
fn sample_with_empty_engine_list_is_a_typed_error_not_a_panic() {
    let result = sample(
        PmuLayout::Xe {
            pmu_type: 42,
            cpu: 0,
            engines: Vec::new(),
        },
        1,
    );
    assert!(
        matches!(result, Err(SampleError::NoEngines(_))),
        "empty xe engine list: got {result:?}"
    );

    let result = sample(
        PmuLayout::I915 {
            pmu_type: 7,
            cpu: 0,
            engines: Vec::new(),
        },
        1,
    );
    assert!(
        matches!(result, Err(SampleError::NoEngines(_))),
        "empty i915 engine list: got {result:?}"
    );
}

/// The `empty_pairs_error` seam folds the two open-phase outcomes: no
/// failure → `NoEngines`; a real failure → the classified open error.
#[test]
fn empty_pairs_error_folds_the_open_phase_honestly() {
    assert!(matches!(empty_pairs_error(None), SampleError::NoEngines(_)));
    let eacces = io::Error::from_raw_os_error(ERR_EACCES);
    assert!(matches!(
        empty_pairs_error(Some(eacces)),
        SampleError::PermissionDenied(_)
    ));
}
