use super::*;
use crate::engine_names::{
    CLASS_COMPUTE, CLASS_COPY, CLASS_RENDER, CLASS_VIDEO, CLASS_VIDEO_ENHANCE,
};

/// Build a fake `/sys` tree and assert the xe PMU + engines are discovered
/// with the kernel-default config packing. Mirrors the on-box Core Ultra
/// layout: bare-mnemonic class dirs spread across gt0 + gt1, a `xe_<BDF>` PMU
/// matched by PCI slot, and format files pinning the shifts.
#[test]
fn discover_xe_layout_from_fixture_matches_on_box_shape() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_priv_helper_xe_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // Device uevent pins the PCI slot for xe PMU resolution.
    std::fs::create_dir_all(root.join("device")).unwrap();
    std::fs::write(
        root.join("device").join("uevent"),
        "DRIVER=xe\nPCI_SLOT_NAME=0000:00:02.0\n",
    )
    .unwrap();
    // Engines under tile0/gt0 + tile0/gt1, bare mnemonics, no busy node.
    for name in ["rcs", "ccs", "bcs"] {
        std::fs::create_dir_all(root.join("device/tile0/gt0/engines").join(name)).unwrap();
    }
    for name in ["vcs", "vecs"] {
        std::fs::create_dir_all(root.join("device/tile0/gt1/engines").join(name)).unwrap();
    }
    // `.defaults` metadata kobject under each GT must be skipped.
    std::fs::create_dir_all(root.join("device/tile0/gt0/engines/.defaults")).unwrap();
    // The xe PMU, matched by PCI slot, with its type + format files.
    let pmu = root.join("event_source/xe_0000_00_02.0");
    std::fs::create_dir_all(pmu.join("format")).unwrap();
    std::fs::write(pmu.join("type"), "42\n").unwrap();
    std::fs::write(pmu.join("format/event"), "config:0-11\n").unwrap();
    std::fs::write(pmu.join("format/engine_instance"), "config:12-19\n").unwrap();
    std::fs::write(pmu.join("format/engine_class"), "config:20-27\n").unwrap();

    let layout =
        discover_pmu_layout_in(&root.join("device"), Driver::Xe, &root.join("event_source"))
            .expect("xe layout discovered");
    let PmuLayout::Xe {
        pmu_type, engines, ..
    } = layout
    else {
        panic!("expected xe layout, got {layout:?}");
    };
    assert_eq!(pmu_type, 42);
    assert_eq!(engines.len(), 5, "five classes across gt0+gt1: {engines:?}");
    let by_class: std::collections::HashMap<u32, &XeEngineCfg> = engines
        .iter()
        .map(|engine| (engine.class, engine))
        .collect();
    // Render: active 0x2, total 0x3 (class 0, instance 0).
    assert_eq!(by_class[&CLASS_RENDER].label, "Render/3D");
    assert_eq!(by_class[&CLASS_RENDER].class_name, "render");
    assert_eq!(by_class[&CLASS_RENDER].instance, 0);
    assert_eq!(by_class[&CLASS_RENDER].active_config, 0x2);
    assert_eq!(by_class[&CLASS_RENDER].total_config, 0x3);
    // Copy: class 1 << 20 → active 0x100002 / total 0x100003.
    assert_eq!(by_class[&CLASS_COPY].active_config, 0x10_0002);
    assert_eq!(by_class[&CLASS_COPY].total_config, 0x10_0003);
    assert_eq!(by_class[&CLASS_COMPUTE].active_config, 0x40_0002);
    assert_eq!(by_class[&CLASS_COMPUTE].total_config, 0x40_0003);
    assert_eq!(by_class[&CLASS_VIDEO].active_config, 0x20_0002);
    assert_eq!(by_class[&CLASS_VIDEO_ENHANCE].active_config, 0x30_0002);

    std::fs::remove_dir_all(root).ok();
}

/// A class repeated under gt0 and gt1 de-duplicates to ONE config pair.
#[test]
fn xe_engine_configs_dedupes_one_class_across_gts() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_priv_helper_dedup_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for gt in ["gt0", "gt1"] {
        std::fs::create_dir_all(root.join("device/tile0").join(gt).join("engines/rcs")).unwrap();
    }
    std::fs::create_dir_all(root.join("device/tile0/gt0/engines/bcs")).unwrap();
    let layout = XeConfigLayout {
        event_shift: XE_DEFAULT_EVENT_SHIFT,
        instance_shift: XE_DEFAULT_INSTANCE_SHIFT,
        class_shift: XE_DEFAULT_CLASS_SHIFT,
    };
    let configs = discover_xe_engine_configs(&root.join("device"), &layout);
    assert_eq!(
        configs.len(),
        2,
        "rcs under gt0+gt1 collapses to one: {configs:?}"
    );
    assert_eq!(
        configs
            .iter()
            .filter(|engine| engine.class == CLASS_RENDER)
            .count(),
        1
    );
    std::fs::remove_dir_all(root).ok();
}

/// i915 discovery builds one config per `(class, instance)` with the
/// `(class<<12)|(instance<<4)|0` encoding.
#[test]
fn discover_i915_layout_encodes_class_and_instance() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_priv_helper_i915_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engines_dir = root.join("device/tile0/gt0/engines");
    for name in ["rcs0", "bcs1", "vecs0"] {
        std::fs::create_dir_all(engines_dir.join(name)).unwrap();
    }
    std::fs::create_dir_all(root.join("event_source/i915")).unwrap();
    std::fs::write(root.join("event_source/i915/type"), "7\n").unwrap();

    let layout = discover_pmu_layout_in(
        &root.join("device"),
        Driver::I915,
        &root.join("event_source"),
    )
    .expect("i915 layout discovered");
    let PmuLayout::I915 {
        pmu_type, engines, ..
    } = layout
    else {
        panic!("expected i915 layout, got {layout:?}");
    };
    assert_eq!(pmu_type, 7);
    let by_label: std::collections::HashMap<&str, &I915EngineCfg> = engines
        .iter()
        .map(|engine| (engine.label.as_str(), engine))
        .collect();
    // Render class 0, instance 0 → 0x0000.
    assert_eq!(by_label["Render/3D"].class, CLASS_RENDER);
    assert_eq!(by_label["Render/3D"].class_name, "render");
    assert_eq!(by_label["Render/3D"].config, 0x0000);
    // Copy class 1 << 12 | instance 1 << 4 → 0x1010.
    assert_eq!(by_label["Copy"].config, 0x1010);
    // Video Encode class 3 << 12 → 0x3000.
    assert_eq!(by_label["Video Encode"].class, CLASS_VIDEO_ENHANCE);
    assert_eq!(by_label["Video Encode"].class_name, "video-enhance");
    assert_eq!(by_label["Video Encode"].config, 0x3000);

    std::fs::remove_dir_all(root).ok();
}

/// GPU discovery scans `/sys/class/drm` in numeric card order and picks the
/// first `xe`/`i915` card, skipping non-Intel cards.
#[test]
fn discover_intel_gpu_picks_first_intel_card_in_numeric_order() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_priv_helper_drm_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // card10 (discrete amd) sorts lexically before card2 by raw name but
    // MUST come after it numerically; card0 is the Intel iGPU.
    for (card, driver) in [("card0", "xe"), ("card2", "amdgpu"), ("card10", "amdgpu")] {
        let device = root.join(card).join("device");
        std::fs::create_dir_all(&device).unwrap();
        std::fs::write(device.join("uevent"), format!("DRIVER={driver}\n")).unwrap();
    }

    let (device, driver) = discover_intel_gpu_in(&root).expect("an Intel card is present");
    assert_eq!(driver, Driver::Xe);
    assert!(
        device.ends_with("card0/device"),
        "lowest Intel card wins: {device:?}"
    );
    std::fs::remove_dir_all(root).ok();
}

/// No Intel card → `None` (no fabrication, no guess at a non-Intel card).
#[test]
fn discover_intel_gpu_returns_none_without_an_intel_card() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_priv_helper_none_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let device = root.join("card0").join("device");
    std::fs::create_dir_all(&device).unwrap();
    std::fs::write(device.join("uevent"), "DRIVER=amdgpu\n").unwrap();
    assert!(discover_intel_gpu_in(&root).is_none());
    std::fs::remove_dir_all(root).ok();
}

/// No PMU (xe card present but no matching `xe_*` PMU registered) → `None`.
#[test]
fn discover_pmu_layout_returns_none_when_xe_pmu_absent() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_priv_helper_nopmu_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("device/tile0/gt0/engines/rcs")).unwrap();
    std::fs::create_dir_all(root.join("event_source")).unwrap(); // empty
    assert!(
        discover_pmu_layout_in(&root.join("device"), Driver::Xe, &root.join("event_source"))
            .is_none()
    );
    std::fs::remove_dir_all(root).ok();
}

/// `pack_engine_busy` with kernel-default shifts reproduces `xe_pmu.c`.
#[test]
fn xe_pack_engine_busy_matches_kernel_default_layout() {
    let layout = XeConfigLayout {
        event_shift: XE_DEFAULT_EVENT_SHIFT,
        instance_shift: XE_DEFAULT_INSTANCE_SHIFT,
        class_shift: XE_DEFAULT_CLASS_SHIFT,
    };
    // Copy (class 1, instance 0): active = 0x100002.
    assert_eq!(
        layout.pack_engine_busy(XE_EVENT_ACTIVE_TICKS, CLASS_COPY, 0),
        0x10_0002
    );
    // Video decode (class 2, instance 3): 0x2 | 3<<12 | 2<<20.
    assert_eq!(
        layout.pack_engine_busy(XE_EVENT_ACTIVE_TICKS, CLASS_VIDEO, 3),
        0x20_3002
    );
}
