use super::*;

#[test]
fn missing_device_root_is_identity_change_but_missing_field_is_unsupported() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_intel_gpu_missing_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    assert_eq!(
        read_intel_gt_frequency(&root).failure,
        Some(FailureKind::IdentityChanged)
    );

    let gt = root.join("tile0/gt0/freq0");
    std::fs::create_dir_all(&gt).expect("fixture frequency directory");
    assert_eq!(
        read_intel_gt_frequency(&root).failure,
        Some(FailureKind::Unsupported)
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn malformed_sibling_does_not_erase_valid_gt_frequency() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_intel_gpu_partial_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    for (gt, value) in [("gt0", "1200\n"), ("gt1", "invalid\n")] {
        let frequency = root.join("tile0").join(gt).join("freq0");
        std::fs::create_dir_all(&frequency).expect("fixture frequency directory");
        std::fs::write(frequency.join("act_freq"), value).expect("fixture frequency");
    }

    let read = read_intel_gt_frequency(&root);
    assert_eq!(read.value, Some(1_200));
    assert_eq!(read.failure, Some(FailureKind::ProviderFault));
    std::fs::remove_dir_all(root).ok();
}

/// `read_intel_gt_engines` discovers the `xe` engine-class tree
/// (`<gt>/engines/<class>/busy`), collapses instances to a stable display
/// label, and keeps the busiest sample per label. Uses ns-scale counter
/// values so the tracker fixture can derive a real percentage afterwards.
#[test]
fn read_intel_gt_engines_discovers_class_tree_and_collapses_labels() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_intel_engines_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    let gt0 = root.join("tile0").join("gt0").join("engines");
    std::fs::create_dir_all(&gt0).expect("fixture engines directory");
    // Four engine classes with cumulative busy-time counters (ns).
    // xe groups by class; create the class dir THEN the busy file.
    for (class, value) in [
        ("render", "0\n"),
        ("copy", "250000000\n"),
        ("video", "0\n"),
        ("video-enhance", "0\n"),
    ] {
        let dir = gt0.join(class);
        std::fs::create_dir_all(&dir).expect("fixture engine class dir");
        std::fs::write(dir.join("busy"), value).expect("fixture busy node");
    }
    // `.defaults` metadata + an unknown future engine must be tolerated.
    std::fs::create_dir_all(gt0.join(".defaults")).expect("fixture defaults kobj");
    std::fs::write(gt0.join(".defaults").join("busy"), "999\n").ok();
    let future = gt0.join("future_unit");
    std::fs::create_dir_all(&future).expect("fixture future engine dir");
    std::fs::write(future.join("busy"), "10\n").expect("fixture future busy node");

    let read = read_intel_gt_engines(&root);
    let failure = read.failure;
    let mut engines = read.value.expect("engines should be discovered");
    engines.sort_by(|left, right| left.name.cmp(&right.name));
    assert_eq!(
        engines
            .iter()
            .map(|engine| engine.name.as_str())
            .collect::<Vec<_>>(),
        [
            "Copy",
            "FUTURE UNIT",
            "Render/3D",
            "Video Decode",
            "Video Encode"
        ],
        "got {engines:?}"
    );
    let by_name: std::collections::HashMap<&str, EngineBusySource> = engines
        .iter()
        .map(|engine| (engine.name.as_str(), engine.busy))
        .collect();
    assert_eq!(by_name["Copy"], EngineBusySource::NanoSeconds(250_000_000));
    assert_eq!(by_name["Render/3D"], EngineBusySource::NanoSeconds(0));
    assert_eq!(by_name["Video Decode"], EngineBusySource::NanoSeconds(0));
    assert_eq!(by_name["Video Encode"], EngineBusySource::NanoSeconds(0));
    assert_eq!(by_name["FUTURE UNIT"], EngineBusySource::NanoSeconds(10));
    assert!(
        failure.is_none(),
        "a fully readable tree must not surface a failure"
    );
    std::fs::remove_dir_all(root).ok();
}

/// When the `engines/` tree is absent (the mainline i915 / unpatched-xe
/// case on this host), the read yields `None` and never panics — the
/// caller keeps `engines` empty instead of fabricating a breakdown.
#[test]
fn read_intel_gt_engines_is_absent_without_engines_tree() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_intel_no_engines_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    // GT exists (so RC6/freq still work) but has no `engines/` child.
    let gt = root.join("tile0").join("gt0").join("freq0");
    std::fs::create_dir_all(&gt).expect("fixture gt directory");

    let read = read_intel_gt_engines(&root);
    assert!(read.value.is_none(), "no engines tree → None: {read:?}");
    // Absent `engines/` is a soft Unsupported, surfaced for diagnostics.
    assert_eq!(read.failure, Some(FailureKind::Unsupported));
    std::fs::remove_dir_all(root).ok();
}

/// The label mapper covers every engine vocabulary the `i915` and `xe`
/// drivers emit, and keeps the encode bucket distinct from decode.
#[test]
fn intel_engine_label_maps_known_and_unknown_names() {
    // xe per-class names.
    assert_eq!(intel_engine_label("render"), "Render/3D");
    assert_eq!(intel_engine_label("copy"), "Copy");
    assert_eq!(intel_engine_label("compute"), "Compute");
    assert_eq!(intel_engine_label("video"), "Video Decode");
    assert_eq!(intel_engine_label("video-enhance"), "Video Encode");
    // i915 per-instance names.
    assert_eq!(intel_engine_label("rcs0"), "Render/3D");
    assert_eq!(intel_engine_label("bcs1"), "Copy");
    assert_eq!(intel_engine_label("ccs2"), "Compute");
    assert_eq!(intel_engine_label("vcs0"), "Video Decode");
    assert_eq!(intel_engine_label("vecs0"), "Video Encode");
    // Unknown future engine: passed through, upper-cased, separators→spaces.
    assert_eq!(intel_engine_label("matrix_unit"), "MATRIX UNIT");
    assert_eq!(intel_engine_label("future-class"), "FUTURE CLASS");
}
