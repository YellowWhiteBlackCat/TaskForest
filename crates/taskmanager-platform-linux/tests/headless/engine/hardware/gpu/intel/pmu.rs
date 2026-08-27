use super::*;

/// Test view of [`super::discover_engine_configs_with_receipt`]: plain vec,
/// empty on unavailable/partial-failure (tests only exercise happy walks).
fn discover_engine_configs(device_path: &std::path::Path) -> Vec<IntelPmuEngine> {
    super::discover_engine_configs_with_receipt(device_path)
        .value
        .unwrap_or_default()
}

/// Test view of [`super::discover_xe_engine_configs_with_receipt`]: plain vec,
/// empty on unavailable/partial-failure (tests only exercise happy walks).
fn discover_xe_engine_configs(
    device_path: &std::path::Path,
    layout: &XeConfigLayout,
) -> Vec<XePmuEngine> {
    super::discover_xe_engine_configs_with_receipt(device_path, layout)
        .value
        .unwrap_or_default()
}

#[test]
fn parse_handles_i915_instance_names_and_xe_class_names() {
    // i915 per-instance names: class + digit instance.
    let rcs0 = parse_engine_instance("rcs0").expect("rcs0");
    assert_eq!(rcs0.label, "Render/3D");
    assert_eq!(rcs0.class, ENGINE_CLASS_RENDER);
    assert_eq!(rcs0.instance, 0);

    let bcs1 = parse_engine_instance("bcs1").expect("bcs1");
    assert_eq!(bcs1.label, "Copy");
    assert_eq!(bcs1.class, ENGINE_CLASS_COPY);
    assert_eq!(bcs1.instance, 1);

    let vecs0 = parse_engine_instance("vecs0").expect("vecs0");
    assert_eq!(vecs0.label, "Video Encode");
    assert_eq!(vecs0.class, ENGINE_CLASS_VIDEO_ENHANCE);

    let vcs2 = parse_engine_instance("vcs2").expect("vcs2");
    assert_eq!(vcs2.label, "Video Decode");
    assert_eq!(vcs2.class, ENGINE_CLASS_VIDEO);
    assert_eq!(vcs2.instance, 2);

    let ccs0 = parse_engine_instance("ccs0").expect("ccs0");
    assert_eq!(ccs0.label, "Compute");
    assert_eq!(ccs0.class, ENGINE_CLASS_COMPUTE);

    // xe per-class names: collapsed to instance 0.
    let render = parse_engine_instance("render").expect("render");
    assert_eq!(render.class, ENGINE_CLASS_RENDER);
    assert_eq!(render.instance, 0);
    assert_eq!(
        parse_engine_instance("video-enhance").map(|p| p.class),
        Some(ENGINE_CLASS_VIDEO_ENHANCE)
    );
    assert_eq!(
        parse_engine_instance("compute").map(|p| p.class),
        Some(ENGINE_CLASS_COMPUTE)
    );
    assert_eq!(
        parse_engine_instance("blitter").map(|p| p.class),
        Some(ENGINE_CLASS_COPY)
    );

    // Unknown / non-digit instance → None (no fabrication).
    assert!(parse_engine_instance("future_unit").is_none());
    assert!(parse_engine_instance("rcs").is_none());
    assert!(parse_engine_instance("rcsX").is_none());
}

/// Verify the i915 busy config encoding end-to-end via the real
/// `discover_engine_configs` walk: `(class << 12) | (instance << 4) |
/// I915_SAMPLE_BUSY(=0)`. Uses a sysfs-shaped fixture so the assertion hits
/// the production parse + shift path, not a hand-computed literal.
#[test]
fn discover_engine_configs_encodes_class_and_instance_like_intel_gpu_top() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_intel_pmu_cfg_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    let engines_dir = root.join("tile0").join("gt0").join("engines");
    for name in ["rcs0", "bcs1", "vecs0"] {
        std::fs::create_dir_all(engines_dir.join(name)).expect("fixture engine dir");
    }
    // `.defaults` metadata must be skipped (no config fabricated for it).
    std::fs::create_dir_all(engines_dir.join(".defaults")).expect("fixture defaults kobj");

    let mut configs = discover_engine_configs(&root);
    configs.sort_by_key(|engine| engine.label.clone());
    let by_label: std::collections::HashMap<&str, &IntelPmuEngine> = configs
        .iter()
        .map(|engine| (engine.label.as_str(), engine))
        .collect();
    // Render class 0, instance 0 → 0x0000.
    assert_eq!(by_label["Render/3D"].class, ENGINE_CLASS_RENDER);
    assert_eq!(by_label["Render/3D"].instance, 0);
    assert_eq!(by_label["Render/3D"].config, 0x0000);
    // Copy class 1 << 12 | instance 1 << 4 → 0x1010.
    assert_eq!(by_label["Copy"].class, ENGINE_CLASS_COPY);
    assert_eq!(by_label["Copy"].instance, 1);
    assert_eq!(by_label["Copy"].config, 0x1010);
    // Video Encode class 3 << 12 → 0x3000.
    assert_eq!(by_label["Video Encode"].class, ENGINE_CLASS_VIDEO_ENHANCE);
    assert_eq!(by_label["Video Encode"].instance, 0);
    assert_eq!(by_label["Video Encode"].config, 0x3000);
    assert_eq!(configs.len(), 3, "no config for .defaults or unknown names");
    std::fs::remove_dir_all(root).ok();
}

// ---- xe config packing (pure math, no PMU open in CI) ------------------

/// `pack_engine_busy` with the kernel-default shifts reproduces the
/// authoritative `xe_pmu.c` encoding for every field, with `function`/`gt`
/// left at zero.
#[test]
fn xe_pack_engine_busy_matches_kernel_default_layout() {
    let layout = XeConfigLayout {
        event_shift: XE_DEFAULT_EVENT_SHIFT,
        instance_shift: XE_DEFAULT_INSTANCE_SHIFT,
        class_shift: XE_DEFAULT_CLASS_SHIFT,
        _function_shift: XE_DEFAULT_FUNCTION_SHIFT,
        _gt_shift: XE_DEFAULT_GT_SHIFT,
    };
    // Render (class 0, instance 0): active = 0x2, total = 0x3.
    assert_eq!(
        layout.pack_engine_busy(XE_PMU_EVENT_ENGINE_ACTIVE_TICKS, 0, 0),
        0x2
    );
    assert_eq!(
        layout.pack_engine_busy(XE_PMU_EVENT_ENGINE_TOTAL_TICKS, 0, 0),
        0x3
    );
    // Copy (class 1, instance 0): event 0x2 | class 1 << 20 = 0x100002.
    assert_eq!(
        layout.pack_engine_busy(XE_PMU_EVENT_ENGINE_ACTIVE_TICKS, ENGINE_CLASS_COPY, 0),
        0x10_0002
    );
    // Video decode (class 2, instance 3): 0x2 | 3 << 12 | 2 << 20.
    assert_eq!(
        layout.pack_engine_busy(XE_PMU_EVENT_ENGINE_ACTIVE_TICKS, ENGINE_CLASS_VIDEO, 3),
        0x20_3002
    );
    // Compute (class 4, instance 1): 0x3 | 1 << 12 | 4 << 20.
    assert_eq!(
        layout.pack_engine_busy(XE_PMU_EVENT_ENGINE_TOTAL_TICKS, ENGINE_CLASS_COMPUTE, 1),
        0x40_1003
    );
    // The function (bits 44-59) and gt (bits 60-63) fields stay zero.
    assert_eq!(
        layout.pack_engine_busy(XE_PMU_EVENT_ENGINE_ACTIVE_TICKS, ENGINE_CLASS_RENDER, 0)
            & (0xFFFFu64 << 44),
        0
    );
}

/// `parse_format_low_bit` reads the low config bit from real-shaped
/// `format/<name>` contents, including comma-separated discontiguous
/// fields, and rejects `>= 64`.
#[test]
fn xe_parse_format_low_bit_handles_xe_layout_files() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_xe_format_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    let format_dir = root.join("format");
    std::fs::create_dir_all(&format_dir).expect("fixture format dir");
    std::fs::write(format_dir.join("event"), "config:0-11\n").expect("event");
    std::fs::write(format_dir.join("engine_instance"), "config:12-19\n").expect("instance");
    std::fs::write(format_dir.join("engine_class"), "config:20-27\n").expect("class");
    // Discontiguous field: still take the LOW end of the first range.
    std::fs::write(format_dir.join("function"), "config:44-59\n").expect("function");
    std::fs::write(format_dir.join("gt"), "config:60-63\n").expect("gt");
    // Whitespace-tolerant and comma-shaped.
    std::fs::write(format_dir.join("comma"), "config: 30-31, 44-45\n").expect("comma");

    assert_eq!(parse_format_low_bit(&root, "event"), Some(0));
    assert_eq!(parse_format_low_bit(&root, "engine_instance"), Some(12));
    assert_eq!(parse_format_low_bit(&root, "engine_class"), Some(20));
    assert_eq!(parse_format_low_bit(&root, "function"), Some(44));
    assert_eq!(parse_format_low_bit(&root, "gt"), Some(60));
    assert_eq!(parse_format_low_bit(&root, "comma"), Some(30));
    // Absent file → None (caller falls back to the kernel default).
    assert_eq!(parse_format_low_bit(&root, "absent"), None);

    std::fs::remove_dir_all(root).ok();
}

/// `parse_xe_config_layout` falls back to the kernel defaults for every
/// absent format file, and adopts the parsed shift when a file is present —
/// pinning the layout against future kernel drift without hard-coding.
#[test]
fn xe_config_layout_uses_parsed_shifts_or_kernel_defaults() {
    // No format files at all → every shift is the kernel default.
    let empty = crate::test_support::repo_temp_dir().join(format!(
        "tm_xe_layout_empty_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&empty).expect("fixture empty pmu");
    let defaulted = parse_xe_config_layout(&empty);
    assert_eq!(defaulted.event_shift, XE_DEFAULT_EVENT_SHIFT);
    assert_eq!(defaulted.instance_shift, XE_DEFAULT_INSTANCE_SHIFT);
    assert_eq!(defaulted.class_shift, XE_DEFAULT_CLASS_SHIFT);
    // And the default layout packs exactly like the kernel.
    assert_eq!(
        defaulted.pack_engine_busy(XE_PMU_EVENT_ENGINE_ACTIVE_TICKS, ENGINE_CLASS_COPY, 5),
        0x10_5002
    );
    std::fs::remove_dir_all(&empty).ok();

    // A drifted class shift (24 instead of 20) is honoured, not overwritten
    // — the whole point of parsing the format files defensively.
    let drifted = crate::test_support::repo_temp_dir().join(format!(
        "tm_xe_layout_drift_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    let dformat = drifted.join("format");
    std::fs::create_dir_all(&dformat).expect("fixture drift format dir");
    std::fs::write(dformat.join("engine_class"), "config:24-31\n").expect("drifted class");
    let parsed = parse_xe_config_layout(&drifted);
    assert_eq!(
        parsed.class_shift, 24,
        "parsed class shift must win over the default"
    );
    // class 1 << 24 (not the default 20): 0x1_000_002.
    assert_eq!(
        parsed.pack_engine_busy(XE_PMU_EVENT_ENGINE_ACTIVE_TICKS, ENGINE_CLASS_COPY, 0),
        0x0100_0002
    );
    std::fs::remove_dir_all(&drifted).ok();
}

/// `discover_xe_engine_configs` walks the GT `engines/` tree and builds an
/// active+total config pair per xe per-class engine, reusing the same sysfs
/// vocabulary as i915 so xe engines line up with the sysfs labels.
#[test]
fn discover_xe_engine_configs_builds_active_and_total_pair() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_xe_engines_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    let engines_dir = root.join("tile0").join("gt0").join("engines");
    for name in ["render", "copy", "video", "video-enhance", "compute"] {
        std::fs::create_dir_all(engines_dir.join(name)).expect("fixture xe engine dir");
    }
    // `.defaults` metadata must be skipped (no config fabricated for it).
    std::fs::create_dir_all(engines_dir.join(".defaults")).expect("fixture defaults kobj");

    let layout = parse_xe_config_layout(&root); // no format files → defaults
    let mut configs = discover_xe_engine_configs(&root, &layout);
    configs.sort_by_key(|engine| engine.label.clone());
    let by_label: std::collections::HashMap<&str, &XePmuEngine> = configs
        .iter()
        .map(|engine| (engine.label.as_str(), engine))
        .collect();
    // Render: active 0x2, total 0x3 (class 0, instance 0).
    assert_eq!(by_label["Render/3D"].active_config, 0x2);
    assert_eq!(by_label["Render/3D"].total_config, 0x3);
    // Copy: class 1 << 20 → active 0x100002 / total 0x100003.
    assert_eq!(by_label["Copy"].active_config, 0x10_0002);
    assert_eq!(by_label["Copy"].total_config, 0x10_0003);
    // Compute: class 4 << 20.
    assert_eq!(by_label["Compute"].active_config, 0x40_0002);
    assert_eq!(by_label["Compute"].total_config, 0x40_0003);
    assert_eq!(
        configs.len(),
        5,
        "no config for .defaults; one per xe engine class"
    );
    std::fs::remove_dir_all(root).ok();
}

/// `parse_xe_engine_instance` accepts the bare class mnemonics the `xe`
/// driver actually registers on Intel Core Ultra (`rcs`/`bcs`/`vcs`/`vecs`/
/// `ccs`, no instance digit) AND the long-form names, all collapsed to
/// instance 0, while rejecting garbage. The bare mnemonic is the exact case
/// the shared i915 parser rejects (`rcs` has no digit) — this is the bug that
/// emptied `snapshot.gpu[0].engines` on the dev box.
#[test]
fn parse_xe_engine_instance_accepts_bare_mnemonics_and_long_forms() {
    // Bare mnemonics (the on-box Core Ultra layout): instance 0.
    let rcs = parse_xe_engine_instance("rcs").expect("bare rcs");
    assert_eq!(rcs.class, ENGINE_CLASS_RENDER);
    assert_eq!(rcs.instance, 0);
    assert_eq!(rcs.label, "Render/3D");

    assert_eq!(
        parse_xe_engine_instance("bcs").map(|p| p.class),
        Some(ENGINE_CLASS_COPY)
    );
    assert_eq!(
        parse_xe_engine_instance("ccs").map(|p| p.class),
        Some(ENGINE_CLASS_COMPUTE)
    );
    // vecs before vcs: encode must not collapse into decode.
    assert_eq!(
        parse_xe_engine_instance("vecs").map(|p| p.class),
        Some(ENGINE_CLASS_VIDEO_ENHANCE)
    );
    assert_eq!(
        parse_xe_engine_instance("vcs").map(|p| p.class),
        Some(ENGINE_CLASS_VIDEO)
    );

    // Long-form names still accepted, instance 0.
    assert_eq!(
        parse_xe_engine_instance("render").map(|p| p.class),
        Some(ENGINE_CLASS_RENDER)
    );
    assert_eq!(
        parse_xe_engine_instance("video-enhance").map(|p| p.class),
        Some(ENGINE_CLASS_VIDEO_ENHANCE)
    );
    // An optional digit tail is tolerated (defensive) but still instance 0.
    let rcs0 = parse_xe_engine_instance("rcs0").expect("defensive rcs0");
    assert_eq!(rcs0.class, ENGINE_CLASS_RENDER);
    assert_eq!(rcs0.instance, 0);

    // Garbage / non-digit tail → None (never fabricated).
    assert!(parse_xe_engine_instance("rcsX").is_none());
    assert!(parse_xe_engine_instance("future_unit").is_none());
    assert!(parse_xe_engine_instance(".defaults").is_none());
}

/// Regression for the on-box Core Ultra layout: engines live under
/// `tile0/gt0/engines/{rcs,ccs,bcs}` and `tile0/gt1/engines/{vcs,vecs}` with
/// CLASS-ONLY names and NO `busy` node. Before the fix the shared i915
/// parser rejected every bare name → empty configs → empty snapshot engines.
/// After the fix, the five distinct classes are enumerated, the configs are
/// packed with the parsed xe shifts, and a class repeated across GTs is
/// de-duplicated to one entry.
#[test]
fn discover_xe_engine_configs_enumerates_bare_mnemonics_across_gts() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_xe_bare_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    // Mirror the real Core Ultra sysfs: bare-mnemonic class dirs, no busy
    // node, spread across gt0 + gt1.
    let gt0 = root.join("tile0").join("gt0").join("engines");
    let gt1 = root.join("tile0").join("gt1").join("engines");
    for name in ["rcs", "ccs", "bcs"] {
        std::fs::create_dir_all(gt0.join(name)).expect("fixture gt0 engine dir");
    }
    for name in ["vcs", "vecs"] {
        std::fs::create_dir_all(gt1.join(name)).expect("fixture gt1 engine dir");
    }
    // `.defaults` metadata kobject under each GT must be skipped.
    std::fs::create_dir_all(gt0.join(".defaults")).expect("fixture gt0 defaults");
    std::fs::create_dir_all(gt1.join(".defaults")).expect("fixture gt1 defaults");

    let layout = parse_xe_config_layout(&root); // no format files → kernel defaults
    let mut configs = discover_xe_engine_configs(&root, &layout);
    configs.sort_by_key(|engine| engine.class);
    assert_eq!(
        configs.len(),
        5,
        "five distinct classes across gt0+gt1, .defaults skipped: {configs:?}"
    );
    let by_class: std::collections::HashMap<u32, &XePmuEngine> = configs
        .iter()
        .map(|engine| (engine.class, engine))
        .collect();
    // Each engine: instance 0, label from the shared vocabulary, and the
    // kernel-default config packing (event | class<<20).
    assert_eq!(by_class[&ENGINE_CLASS_RENDER].label, "Render/3D");
    assert_eq!(by_class[&ENGINE_CLASS_RENDER].instance, 0);
    assert_eq!(by_class[&ENGINE_CLASS_RENDER].active_config, 0x2);
    assert_eq!(by_class[&ENGINE_CLASS_RENDER].total_config, 0x3);
    assert_eq!(by_class[&ENGINE_CLASS_COPY].active_config, 0x10_0002);
    assert_eq!(by_class[&ENGINE_CLASS_COPY].total_config, 0x10_0003);
    assert_eq!(by_class[&ENGINE_CLASS_COMPUTE].active_config, 0x40_0002);
    assert_eq!(by_class[&ENGINE_CLASS_COMPUTE].total_config, 0x40_0003);
    assert_eq!(by_class[&ENGINE_CLASS_VIDEO].active_config, 0x20_0002);
    assert_eq!(by_class[&ENGINE_CLASS_VIDEO].total_config, 0x20_0003);
    assert_eq!(
        by_class[&ENGINE_CLASS_VIDEO_ENHANCE].active_config,
        0x30_0002
    );
    assert_eq!(
        by_class[&ENGINE_CLASS_VIDEO_ENHANCE].total_config,
        0x30_0003
    );

    std::fs::remove_dir_all(root).ok();
}

/// A class repeated under both `gt0` and `gt1` is de-duplicated to ONE
/// config — the xe PMU counts system-wide per class, so two identical opens
/// would be a double count (and a duplicate label collision in the tracker).
#[test]
fn discover_xe_engine_configs_dedupes_one_class_across_gts() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_xe_dedup_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    // `rcs` under BOTH gt0 and gt1 — plus a distinct class to keep the walk
    // non-trivial.
    for gt in ["gt0", "gt1"] {
        let dir = root.join("tile0").join(gt).join("engines").join("rcs");
        std::fs::create_dir_all(dir).expect("fixture rcs engine dir");
    }
    std::fs::create_dir_all(root.join("tile0").join("gt0").join("engines").join("bcs"))
        .expect("fixture bcs engine dir");

    let layout = parse_xe_config_layout(&root);
    let configs = discover_xe_engine_configs(&root, &layout);
    assert_eq!(
        configs.len(),
        2,
        "rcs under gt0+gt1 collapses to one; bcs is the second: {configs:?}"
    );
    assert_eq!(
        configs
            .iter()
            .filter(|e| e.class == ENGINE_CLASS_RENDER)
            .count(),
        1
    );
    std::fs::remove_dir_all(root).ok();
}
