use super::*;

#[test]
fn provider_pci_identity_normalizes_to_linux_sysfs_shape() {
    assert_eq!(
        normalize_pci_slot("00000000:01:00.0"),
        Some("0000:01:00.0".to_string())
    );
    assert_eq!(
        normalize_pci_slot("00000000:AF:1F.7"),
        Some("0000:af:1f.7".to_string())
    );
    assert_eq!(normalize_pci_slot("not-a-pci-id"), None);
    assert_eq!(normalize_pci_slot("0000:01:20.0"), None);
}

#[test]
fn a_sparse_secondary_identity_provider_does_not_erase_richer_pci_identity() {
    let device_id = stable_gpu_id("card0", Some("0000:00:02.0"));
    let mut baseline = GpuMetrics::new(device_id.clone(), "Intel");
    baseline.marketing_name = Some("Arc B390".into());
    baseline.pci_vendor_id = Some(0x8086);
    baseline.pci_device_id = Some(0xB080);
    baseline.pci_slot = Some("0000:00:02.0".into());
    let mut gpus = vec![baseline];
    merge_provider_samples(
        &mut gpus,
        taskmanager_core::ProviderId::borrowed("fixture.sparse-identity"),
        vec![GpuProviderSample {
            metrics: GpuMetrics::new(device_id, "Intel"),
            fields: vec![GpuMetricField::Identity],
            field_failures: Vec::new(),
        }],
    );

    assert_eq!(gpus[0].marketing_name.as_deref(), Some("Arc B390"));
    assert_eq!(gpus[0].pci_vendor_id, Some(0x8086));
    assert_eq!(gpus[0].pci_device_id, Some(0xB080));
    assert_eq!(gpus[0].pci_slot.as_deref(), Some("0000:00:02.0"));
}

#[test]
fn procfs_fallback_keeps_identical_nvidia_boards_distinct() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_nvidia_procfs_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for slot in ["0000:01:00.0", "0000:02:00.0"] {
        let directory = root.join("gpus").join(slot);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("information"), "Model: NVIDIA RTX\n").unwrap();
    }

    let mut gpus = Vec::new();
    append_nvidia_procfs(&root, &mut gpus);
    append_nvidia_procfs(&root, &mut gpus);

    assert_eq!(gpus.len(), 2);
    assert_ne!(gpus[0].device_id, gpus[1].device_id);
    assert!(gpus.iter().all(|gpu| gpu.brand == "NVIDIA RTX"));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn nvml_sample_enriches_matching_pci_device_without_duplicate() {
    let device_id = stable_gpu_id("card0", Some("0000:01:00.0"));
    let mut base = GpuMetrics::new(device_id.clone(), "NVIDIA");
    base.driver = Some("nvidia".into());
    let mut gpus = vec![base];
    let mut enriched = GpuMetrics::new(device_id, "NVIDIA RTX");
    enriched.device_state = DeviceState::healthy(100);
    enriched.engines = vec![GpuEngine {
        name: "Video Encode".into(),
        kind: GpuEngineKind::VideoEncode,
        usage_pct: 7.0,
    }];
    enriched.driver = Some("nvidia".into());
    enriched.driver_version = Some("566.36".into());
    enriched.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(42.0, 100),
        memory_used_bytes: ScalarObservation::available(4, 100),
        memory_total_bytes: ScalarObservation::available(16, 100),
        dedicated_vram_used_bytes: ScalarObservation::available(4, 100),
        dedicated_vram_total_bytes: ScalarObservation::available(16, 100),
        temperature_c: ScalarObservation::available(61.0, 100),
        power_w: ScalarObservation::available(125.0, 100),
        frequency_mhz: ScalarObservation::available(2_100, 100),
        max_frequency_mhz: ScalarObservation::available(2_700, 100),
        ..Default::default()
    });
    merge_provider_samples(
        &mut gpus,
        taskmanager_core::ProviderId::borrowed("fixture.nvml"),
        vec![GpuProviderSample {
            metrics: enriched,
            fields: vec![
                GpuMetricField::Brand,
                GpuMetricField::Utilization,
                GpuMetricField::Memory,
                GpuMetricField::Temperature,
                GpuMetricField::Power,
                GpuMetricField::Frequency,
                GpuMetricField::Engines,
                GpuMetricField::Driver,
                GpuMetricField::DriverVersion,
            ],
            field_failures: Vec::new(),
        }],
    );

    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].brand, "NVIDIA RTX");
    assert_eq!(gpus[0].current_utilization_pct(), Some(42.0));
    assert_eq!(gpus[0].current_memory_total_bytes(), Some(16));
    assert_eq!(gpus[0].current_temperature_c(), Some(61.0));
    assert_eq!(gpus[0].current_frequency_mhz(), Some(2_100));
    assert_eq!(gpus[0].current_max_frequency_mhz(), Some(2_700));
    assert_eq!(gpus[0].engines.len(), 1);
    assert_eq!(gpus[0].engines[0].kind, GpuEngineKind::VideoEncode);
    // The driver name and its version merge as separate typed fields.
    assert_eq!(gpus[0].driver.as_deref(), Some("nvidia"));
    assert_eq!(gpus[0].driver_version.as_deref(), Some("566.36"));
}

/// `parse_busy_percent` is the one pure-parsing helper introduced for the
/// per-engine / aggregate util read. It must accept plain integers with or
/// without trailing whitespace/newline, reject garbage, and clamp out-of-range.
/// These tests do NOT touch a real GPU — they pass mock sysfs strings.
mod parse_busy_percent {
    use super::super::parse_busy_percent;

    #[test]
    fn plain_integer() {
        assert_eq!(parse_busy_percent("42"), Some(42.0));
    }

    #[test]
    fn trailing_newline_and_spaces_are_trimmed() {
        // sysfs reads almost always come back with a trailing "\n".
        assert_eq!(parse_busy_percent("73\n"), Some(73.0));
        assert_eq!(parse_busy_percent("  15 \n"), Some(15.0));
    }

    #[test]
    fn zero_and_max_boundaries_pass_through() {
        assert_eq!(parse_busy_percent("0"), Some(0.0));
        assert_eq!(parse_busy_percent("100"), Some(100.0));
    }

    #[test]
    fn over_one_hundred_is_clamped() {
        // amdgpu shouldn't emit >100, but defensively clamp rather than panic.
        assert_eq!(parse_busy_percent("137"), Some(100.0));
        assert_eq!(parse_busy_percent("100.0"), Some(100.0));
    }

    #[test]
    fn negative_is_clamped_to_zero() {
        assert_eq!(parse_busy_percent("-5"), Some(0.0));
    }

    #[test]
    fn empty_or_whitespace_is_none() {
        assert_eq!(parse_busy_percent(""), None);
        assert_eq!(parse_busy_percent("   \n"), None);
    }

    #[test]
    fn non_numeric_is_none() {
        assert_eq!(parse_busy_percent("n/a"), None);
        assert_eq!(parse_busy_percent("cat"), None);
    }
}

/// `read_amdgpu_engines` dynamically walks every `*_busy_percent` node.
/// Known semantic names retain stable labels while a future unknown engine
/// is kept rather than rejected by an allowlist.
#[test]
fn read_amdgpu_engines_picks_up_only_present_nodes() {
    let tmp = crate::test_support::repo_temp_dir().join(format!(
        "tm_gpu_engines_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    // Only gfx + dec present; the other seven nodes are absent.
    std::fs::write(tmp.join("gfx_busy_percent"), "64\n").unwrap();
    std::fs::write(tmp.join("dec_busy_percent"), "12\n").unwrap();
    // Negative / over-range nodes get clamped, not dropped.
    std::fs::write(tmp.join("enc_busy_percent"), "-3\n").unwrap();
    std::fs::write(tmp.join("compute_busy_percent"), "250\n").unwrap();
    std::fs::write(tmp.join("future_media_busy_percent"), "33\n").unwrap();
    // Aggregate nodes are owned by other fields, not the engine list.
    std::fs::write(tmp.join("gpu_busy_percent"), "42\n").unwrap();
    std::fs::write(tmp.join("mem_busy_percent"), "24\n").unwrap();

    let mut engines = read_amdgpu_engines(&tmp);
    engines.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(engines.len(), 5, "expected 5 engines, got {engines:?}");
    let by_name: std::collections::HashMap<&str, f32> = engines
        .iter()
        .map(|e| (e.name.as_str(), e.usage_pct))
        .collect();
    assert_eq!(by_name.get("Graphics (3D)"), Some(&64.0));
    assert_eq!(by_name.get("Video Decode"), Some(&12.0));
    assert_eq!(by_name.get("Video Encode"), Some(&0.0), "negative clamped");
    assert_eq!(by_name.get("Compute"), Some(&100.0), "over-100 clamped");
    assert_eq!(by_name.get("FUTURE MEDIA"), Some(&33.0));
    assert_eq!(
        engines
            .iter()
            .find(|engine| engine.name == "Video Decode")
            .map(|engine| engine.kind),
        Some(GpuEngineKind::VideoDecode)
    );
    assert_eq!(
        engines
            .iter()
            .find(|engine| engine.name == "FUTURE MEDIA")
            .map(|engine| engine.kind),
        Some(GpuEngineKind::Unknown)
    );
    // Absent engines must NOT appear.
    assert!(!by_name.contains_key("Memory (Copy)"));
    assert!(!by_name.contains_key("JPEG"));
    assert!(!by_name.contains_key("GPU"));
    assert!(!by_name.contains_key("MEM"));

    std::fs::remove_dir_all(&tmp).ok();
}

/// On a vendor/driver that exposes no per-engine nodes (this host: Intel
/// `xe`), `read_amdgpu_engines` must return an empty vec, never panic.
#[test]
fn read_amdgpu_engines_empty_when_no_nodes() {
    let tmp = crate::test_support::repo_temp_dir().join(format!(
        "tm_gpu_engines_empty_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let engines = read_amdgpu_engines(&tmp);
    assert!(
        engines.is_empty(),
        "expected empty engine list, got {engines:?}"
    );
    std::fs::remove_dir_all(&tmp).ok();
}

/// End-to-end: `detect_gpu_metrics_from_paths` against a synthetic DRM
/// tree modelled on AMD amdgpu verifies the VRAM split (dedicated +
/// shared + back-compat alias) and engine wiring all flow through.
#[test]
fn detect_amdgpu_like_card_populates_split_and_engines() {
    let drm = crate::test_support::repo_temp_dir().join(format!(
        "tm_drm_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let card = drm.join("card0").join("device");
    std::fs::create_dir_all(&card).unwrap();
    // Vendor 0x1002 = AMD.
    std::fs::write(card.join("vendor"), "0x1002\n").unwrap();
    std::fs::write(card.join("gpu_busy_percent"), "57\n").unwrap();
    // Dedicated VRAM: 8 GiB total, 1 GiB used.
    std::fs::write(
        card.join("mem_info_vram_total"),
        (8u64 * 1024 * 1024 * 1024).to_string(),
    )
    .unwrap();
    std::fs::write(
        card.join("mem_info_vram_used"),
        (1024 * 1024 * 1024).to_string(),
    )
    .unwrap();
    // Shared GTT: 16 GiB total, 256 MiB used.
    std::fs::write(
        card.join("mem_info_gtt_total"),
        (16u64 * 1024 * 1024 * 1024).to_string(),
    )
    .unwrap();
    std::fs::write(
        card.join("mem_info_gtt_used"),
        (256u64 * 1024 * 1024).to_string(),
    )
    .unwrap();
    // Two engines present.
    std::fs::write(card.join("gfx_busy_percent"), "57\n").unwrap();
    std::fs::write(card.join("dec_busy_percent"), "8\n").unwrap();

    // `card0` needs to be a directory entry under drm whose name starts
    // with "card" and contains no "-". The intermediate `card0/device`
    // path is what detect scans, so card0 itself just has to exist.
    let nvidia = crate::test_support::repo_temp_dir().join("does_not_exist_in_this_test");
    let modules = crate::test_support::repo_temp_dir().join("does_not_exist_in_this_test");
    let gpus = detect_gpu_metrics_from_paths(&drm, &nvidia, &modules);
    assert_eq!(gpus.len(), 1, "got {gpus:?}");
    let g = &gpus[0];
    assert_eq!(g.brand, "AMD");
    assert_eq!(g.current_utilization_pct(), Some(57.0));
    // Dedicated split.
    assert_eq!(
        g.current_dedicated_vram_total_bytes(),
        Some(8 * 1024 * 1024 * 1024)
    );
    assert_eq!(
        g.current_dedicated_vram_used_bytes(),
        Some(1024 * 1024 * 1024)
    );
    // Shared split.
    assert_eq!(
        g.current_shared_vram_total_bytes(),
        Some(16 * 1024 * 1024 * 1024)
    );
    assert_eq!(g.current_shared_vram_used_bytes(), Some(256 * 1024 * 1024));
    // Engines.
    assert_eq!(g.engines.len(), 2, "got {:?}", g.engines);
    assert!(
        g.engines
            .iter()
            .any(|e| e.name == "Graphics (3D)" && (e.usage_pct - 57.0).abs() < 1e-6)
    );
    assert!(
        g.engines
            .iter()
            .any(|e| e.name == "Video Decode" && (e.usage_pct - 8.0).abs() < 1e-6)
    );

    std::fs::remove_dir_all(&drm).ok();
}

/// On an Intel `xe`-style card that exposes vendor but NONE of the busy /
/// mem_info nodes (the situation on THIS host), detection must still
/// return one GPU with all-zero VRAM, an empty engine list, and a
/// sensible brand — never panic, never skip the card.
#[test]
fn detect_intel_xe_like_card_has_zero_vram_and_no_engines() {
    let drm = crate::test_support::repo_temp_dir().join(format!(
        "tm_drm_intel_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let card = drm.join("card1").join("device");
    std::fs::create_dir_all(&card).unwrap();
    std::fs::write(card.join("vendor"), "0x8086\n").unwrap();
    // No gpu_busy_percent, no mem_info_*, no *_busy_percent — exactly this host.

    let nvidia = crate::test_support::repo_temp_dir().join("does_not_exist_in_this_test");
    let modules = crate::test_support::repo_temp_dir().join("does_not_exist_in_this_test");
    let gpus = detect_gpu_metrics_from_paths(&drm, &nvidia, &modules);
    assert_eq!(gpus.len(), 1, "got {gpus:?}");
    let g = &gpus[0];
    // No `driver` symlink in this synthetic tree → None; the brand composer
    // falls back to "Intel Graphics" for an unknown Intel driver.
    assert_eq!(g.brand, "Intel Graphics");
    assert_eq!(g.driver, None);
    assert_eq!(g.current_utilization_pct(), None);
    assert_eq!(g.current_dedicated_vram_used_bytes(), None);
    assert_eq!(g.current_dedicated_vram_total_bytes(), None);
    assert_eq!(g.current_shared_vram_used_bytes(), None);
    assert_eq!(g.current_shared_vram_total_bytes(), None);
    assert!(
        g.engines.is_empty(),
        "Intel xe has no per-engine sysfs: {g:?}"
    );
    // No tile*/gt*/freq0/act_freq in this synthetic tree → no frequency.
    assert_eq!(g.current_frequency_mhz(), None);
    assert_eq!(g.current_temperature_c(), None);
    assert_eq!(g.current_power_w(), None);
    assert_eq!(g.current_idle_residency_pct(), None);
    assert_eq!(g.current_memory_used_bytes(), None);
    assert_eq!(g.current_memory_total_bytes(), None);

    std::fs::remove_dir_all(&drm).ok();
}

/// `detect_gpu_metrics_with_rc6_from_paths` derives a REAL Intel i915/xe
/// usage from the monotonic `gtidle/idle_residency_ms` counter, using the
/// same prev-tick rate pattern as network rx/tx. This synthesises the xe GT
/// tree (verified shape on this host: `tile0/gt0/gtidle/idle_residency_ms`)
/// and asserts:
///   - tick 1 (no prev): usage stays 0.0,
///   - tick 2 over +1s with +100 ms residency → RC6=10% → usage=90%,
///   - `act_freq` flows through to `gpu_freq_mhz`.
#[test]
fn detect_intel_rc6_derives_usage_from_residency_delta() {
    let drm = crate::test_support::repo_temp_dir().join(format!(
        "tm_drm_rc6_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let card = drm.join("card0").join("device");
    let gtidle = card.join("tile0").join("gt0").join("gtidle");
    let freq = card.join("tile0").join("gt0").join("freq0");
    std::fs::create_dir_all(&gtidle).unwrap();
    std::fs::create_dir_all(&freq).unwrap();
    std::fs::write(card.join("vendor"), "0x8086\n").unwrap();
    std::fs::write(gtidle.join("idle_residency_ms"), "0\n").unwrap();
    std::fs::write(freq.join("act_freq"), "1300\n").unwrap();

    let nvidia = crate::test_support::repo_temp_dir().join("does_not_exist_in_this_test");
    let modules = crate::test_support::repo_temp_dir().join("does_not_exist_in_this_test");
    let mut prev: std::collections::HashMap<String, (u64, std::time::Instant)> =
        std::collections::HashMap::new();

    // Tick 1: no prev entry → usage unchanged (0.0 for xe).
    let t0 = std::time::Instant::now();
    let g1 = detect_gpu_metrics_with_rc6_from_paths(&drm, &nvidia, &modules, &mut prev, t0);
    assert_eq!(g1.len(), 1);
    assert_eq!(g1[0].brand, "Intel Graphics");
    assert_eq!(g1[0].current_utilization_pct(), None);
    assert_eq!(g1[0].current_frequency_mhz(), Some(1300));
    assert_eq!(g1[0].current_max_frequency_mhz(), None);
    // prev got seeded.
    assert_eq!(
        prev.len(),
        1,
        "RC6 prev state should be seeded after tick 1"
    );

    // Tick 2: simulate +100 ms RC6 residency over +1 s → RC6 = 10% → 90% busy.
    std::fs::write(gtidle.join("idle_residency_ms"), "100\n").unwrap();
    let t1 = t0 + std::time::Duration::from_secs(1);
    let g2 = detect_gpu_metrics_with_rc6_from_paths(&drm, &nvidia, &modules, &mut prev, t1);
    assert_eq!(g2.len(), 1);
    assert!(
        (g2[0].current_utilization_pct().unwrap_or_default() - 90.0).abs() < 0.5,
        "expected ~90% usage, got {}",
        g2[0].current_utilization_pct().unwrap_or_default()
    );
    assert!(
        g2[0]
            .current_utilization_pct()
            .is_some_and(|value| (value - 90.0).abs() < 0.5)
    );
    assert_eq!(g2[0].current_idle_residency_pct(), Some(10.0));

    // Tick 3: 100% idle over the next second (full RC6) → 0% usage.
    std::fs::write(gtidle.join("idle_residency_ms"), "1100\n").unwrap();
    let t2 = t1 + std::time::Duration::from_secs(1);
    let g3 = detect_gpu_metrics_with_rc6_from_paths(&drm, &nvidia, &modules, &mut prev, t2);
    assert!(
        g3[0]
            .current_utilization_pct()
            .is_some_and(|value| value.abs() < 0.5),
        "expected ~0% usage at full RC6, got {}",
        g3[0].current_utilization_pct().unwrap_or_default()
    );
    assert_eq!(g3[0].current_idle_residency_pct(), Some(100.0));

    std::fs::remove_dir_all(&drm).ok();
}

// ── pure helpers introduced for the Intel `xe` data sourcing ────────────

/// `parse_driver_name` takes the basename of a `device/driver` symlink
/// target. It must tolerate trailing whitespace, a trailing slash, and
/// arbitrary depth; empty/garbage input yields `None`.
mod parse_driver_name {
    use super::super::parse_driver_name;

    #[test]
    fn basename_of_relative_xe_link() {
        assert_eq!(
            parse_driver_name("../../../../bus/pci/drivers/xe"),
            Some("xe".to_string())
        );
    }

    #[test]
    fn basename_of_absolute_amdgpu_link() {
        assert_eq!(
            parse_driver_name("/sys/bus/pci/drivers/amdgpu"),
            Some("amdgpu".to_string())
        );
    }

    #[test]
    fn trailing_slash_and_whitespace_are_trimmed() {
        assert_eq!(
            parse_driver_name("  /sys/bus/pci/drivers/i915/  "),
            Some("i915".to_string())
        );
    }

    #[test]
    fn empty_or_root_is_none() {
        assert_eq!(parse_driver_name(""), None);
        assert_eq!(parse_driver_name("   "), None);
        assert_eq!(parse_driver_name("/"), None);
    }
}

/// `compose_intel_brand` maps the bound driver to a dep-free model string.
/// `xe` is the differentiator (newer integrated Xe LPG parts); everything
/// else Intel falls back to the legacy "Intel Graphics".
#[test]
fn compose_intel_brand_maps_by_driver() {
    assert_eq!(compose_intel_brand(Some("xe")), "Intel Xe Graphics");
    assert_eq!(compose_intel_brand(Some("i915")), "Intel Graphics");
    assert_eq!(compose_intel_brand(Some("anything_else")), "Intel Graphics");
    assert_eq!(compose_intel_brand(None), "Intel Graphics");
}

/// `read_intel_gt_freq_mhz` must fall back to `cur_freq` per tile when
/// `act_freq` reads 0 — the verified `xe` idle state on this host
/// (`act_freq=0`, `cur_freq=900`). Models two GTs: gt0 idle (act=0/cur=900),
/// gt1 active (act=1300/cur=1300) → per-tile max picks 1300. Also verifies
/// `max_freq` reads `freq0/max_freq`.
#[test]
fn read_intel_gt_freq_falls_back_to_cur_freq_when_act_is_zero() {
    let drm = crate::test_support::repo_temp_dir().join(format!(
        "tm_drm_freq_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let card = drm.join("card0").join("device");
    let gt0f = card.join("tile0").join("gt0").join("freq0");
    let gt1f = card.join("tile0").join("gt1").join("freq0");
    std::fs::create_dir_all(&gt0f).unwrap();
    std::fs::create_dir_all(&gt1f).unwrap();
    // gt0: RC6 clock-gated (act=0), cur_freq=900 — the fallback must kick in.
    std::fs::write(gt0f.join("act_freq"), "0\n").unwrap();
    std::fs::write(gt0f.join("cur_freq"), "900\n").unwrap();
    // gt1: actively clocked.
    std::fs::write(gt1f.join("act_freq"), "1300\n").unwrap();
    std::fs::write(gt1f.join("cur_freq"), "1300\n").unwrap();
    // Max clock from gt0 (2500); gt1 lower (1200) → per-tile max 2500.
    std::fs::write(gt0f.join("max_freq"), "2500\n").unwrap();
    std::fs::write(gt1f.join("max_freq"), "1200\n").unwrap();

    assert_eq!(read_intel_gt_freq_mhz(&card), Some(1300));
    assert_eq!(read_intel_gt_max_freq_mhz(&card), Some(2500));

    std::fs::remove_dir_all(&drm).ok();
}

/// When EVERY GT reports `act_freq=0` and there's no `cur_freq` fallback,
/// the reader must return `None` (don't surface a misleading 0 MHz).
#[test]
fn read_intel_gt_freq_none_when_all_zero_and_no_cur_freq() {
    let drm = crate::test_support::repo_temp_dir().join(format!(
        "tm_drm_freq0_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let card = drm.join("card0").join("device");
    let gt0f = card.join("tile0").join("gt0").join("freq0");
    std::fs::create_dir_all(&gt0f).unwrap();
    std::fs::write(gt0f.join("act_freq"), "0\n").unwrap();
    // No cur_freq node at all.
    assert_eq!(read_intel_gt_freq_mhz(&card), None);
    std::fs::remove_dir_all(&drm).ok();
}

/// End-to-end: a synthetic `xe` card with a `driver` symlink is recognised
/// as "Intel Xe Graphics" and the driver field is populated — the brand fix
/// that unbreaks this host's previously-generic GPU heading.
#[cfg(unix)]
#[test]
fn detect_intel_xe_card_uses_driver_for_brand() {
    let drm = crate::test_support::repo_temp_dir().join(format!(
        "tm_drm_xebrand_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let card = drm.join("card0").join("device");
    std::fs::create_dir_all(&card).unwrap();
    std::fs::write(card.join("vendor"), "0x8086\n").unwrap();
    // Fake a driver symlink target pointing at a directory we control; the
    // reader follows read_link and takes the basename. We symlink to a
    // path ending in "xe".
    let fake_drv = drm.join("fake_drivers").join("xe");
    std::fs::create_dir_all(&fake_drv).unwrap();
    std::os::unix::fs::symlink(&fake_drv, card.join("driver")).unwrap();

    let nvidia = crate::test_support::repo_temp_dir().join("does_not_exist_in_this_test");
    let modules = crate::test_support::repo_temp_dir().join("does_not_exist_in_this_test");
    let gpus = detect_gpu_metrics_from_paths(&drm, &nvidia, &modules);
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].brand, "Intel Xe Graphics");
    assert_eq!(gpus[0].driver.as_deref(), Some("xe"));

    std::fs::remove_dir_all(&drm).ok();
}

/// A driver module that declares its own release (the out-of-tree `nvidia`
/// module carries `/sys/module/nvidia/version`) fills `driver_version`, while
/// an in-tree DRM driver (`radeon`, `nouveau`) ships with the kernel and
/// exposes no module version — its absence must stay honest (`None`), never
/// the kernel release misfiled as a driver version. Also proves the DRI driver
/// name and vendor brand thread through for the nouveau/radeon gap families.
#[cfg(unix)]
#[test]
fn module_declared_driver_version_fills_only_versioned_modules() {
    let base = crate::test_support::repo_temp_dir().join(format!(
        "tm_drm_modver_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let drm = base.join("drm");
    let modules = base.join("module");

    // card0: NVIDIA vendor with the versioned out-of-tree `nvidia` module.
    let nvidia_card = drm.join("card0").join("device");
    std::fs::create_dir_all(&nvidia_card).unwrap();
    std::fs::write(nvidia_card.join("vendor"), "0x10de\n").unwrap();
    std::fs::write(nvidia_card.join("uevent"), "PCI_SLOT_NAME=0000:01:00.0\n").unwrap();
    let nvidia_module_dir = modules.join("nvidia");
    std::fs::create_dir_all(&nvidia_module_dir).unwrap();
    std::fs::write(nvidia_module_dir.join("version"), "550.90.07\n").unwrap();
    // The `driver` symlink's basename names the kernel driver, so the fixture
    // link points at a directory ending in `nvidia`.
    std::os::unix::fs::symlink(&nvidia_module_dir, nvidia_card.join("driver")).unwrap();

    // card1/card2: AMD and NVIDIA boards bound to the in-tree `radeon` /
    // `nouveau` drivers, which declare no module version of their own.
    for (card, vendor, driver, slot) in [
        ("card1", "0x1002\n", "radeon", "0000:02:00.0\n"),
        ("card2", "0x10de\n", "nouveau", "0000:03:00.0\n"),
    ] {
        let device = drm.join(card).join("device");
        std::fs::create_dir_all(&device).unwrap();
        std::fs::write(device.join("vendor"), vendor).unwrap();
        std::fs::write(device.join("uevent"), format!("PCI_SLOT_NAME={slot}")).unwrap();
        let driver_dir = base.join("drivers").join(driver);
        std::fs::create_dir_all(&driver_dir).unwrap();
        std::os::unix::fs::symlink(&driver_dir, device.join("driver")).unwrap();
    }

    let nvidia = crate::test_support::repo_temp_dir().join("does_not_exist_in_this_test");
    let gpus = detect_gpu_metrics_from_paths(&drm, &nvidia, &modules);
    assert_eq!(gpus.len(), 3, "got {gpus:?}");
    let by_driver: std::collections::HashMap<&str, &GpuMetrics> = gpus
        .iter()
        .map(|gpu| (gpu.driver.as_deref().unwrap_or_default(), gpu))
        .collect();

    let nvidia_gpu = by_driver["nvidia"];
    assert_eq!(nvidia_gpu.brand, "NVIDIA");
    assert_eq!(nvidia_gpu.driver_version.as_deref(), Some("550.90.07"));
    let radeon_gpu = by_driver["radeon"];
    assert_eq!(radeon_gpu.brand, "AMD");
    assert_eq!(
        radeon_gpu.driver_version, None,
        "in-tree radeon declares no module version"
    );
    let nouveau_gpu = by_driver["nouveau"];
    assert_eq!(nouveau_gpu.brand, "NVIDIA");
    assert_eq!(
        nouveau_gpu.driver_version, None,
        "in-tree nouveau declares no module version"
    );

    std::fs::remove_dir_all(&base).ok();
}

/// `/proc/driver/nvidia/version` is a system-wide procfs fact: the NVRM
/// release it declares attaches to every board found under the procfs GPU
/// tree, without NVML. A missing or unparseable file keeps the field absent.
#[cfg(unix)]
#[test]
fn nvidia_procfs_nvrm_release_attaches_to_every_board() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_nvidia_procfs_version_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for slot in ["0000:01:00.0", "0000:02:00.0"] {
        let directory = root.join("gpus").join(slot);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("information"), "Model: NVIDIA RTX\n").unwrap();
    }
    std::fs::write(
        root.join("version"),
        "NVRM version: NVIDIA UNIX x86_64 Kernel Module  550.107.02  Tue Oct 15 12:50:29 UTC 2024\n\
         GCC version:  gcc version 14.2.1\n",
    )
    .unwrap();

    let mut gpus = Vec::new();
    append_nvidia_procfs(&root, &mut gpus);

    assert_eq!(gpus.len(), 2, "got {gpus:?}");
    assert!(
        gpus.iter()
            .all(|gpu| gpu.driver_version.as_deref() == Some("550.107.02"))
    );

    std::fs::remove_dir_all(root).ok();
}

/// `parse_module_version` reports the module's declared release verbatim from
/// the first line and treats empty nodes as an honest absence.
mod parse_module_version {
    use super::super::parse_module_version;

    #[test]
    fn single_line_release_is_kept_verbatim() {
        assert_eq!(
            parse_module_version("550.90.07\n"),
            Some("550.90.07".into())
        );
        assert_eq!(parse_module_version("  3.2.1  \n"), Some("3.2.1".into()));
    }

    #[test]
    fn only_the_first_line_counts() {
        assert_eq!(parse_module_version("1.2\nignored\n"), Some("1.2".into()));
    }

    #[test]
    fn empty_or_blank_node_is_none() {
        assert_eq!(parse_module_version(""), None);
        assert_eq!(parse_module_version("   \n"), None);
        assert_eq!(parse_module_version("\n"), None);
    }
}

/// `parse_nvrm_driver_version` extracts the release token from the NVRM prose
/// line and rejects files without one.
mod parse_nvrm_driver_version {
    use super::super::parse_nvrm_driver_version;

    #[test]
    fn release_token_is_extracted_from_prose() {
        assert_eq!(
            parse_nvrm_driver_version(
                "NVRM version: NVIDIA UNIX x86_64 Kernel Module  550.107.02  Tue Oct 15 12:50:29 UTC 2024\n"
            )
            .as_deref(),
            Some("550.107.02")
        );
        assert_eq!(
            parse_nvrm_driver_version(
                "NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  565.57.01  Release Build  (root@builder)\n"
            )
            .as_deref(),
            Some("565.57.01")
        );
    }

    #[test]
    fn sibling_lines_are_ignored() {
        assert_eq!(
            parse_nvrm_driver_version(
                "Platform: x86_64\nNVRM version: NVIDIA UNIX x86_64 Kernel Module  470.94  Wed\nGCC version:  gcc\n"
            )
            .as_deref(),
            Some("470.94")
        );
    }

    #[test]
    fn missing_or_versionless_text_is_none() {
        assert_eq!(parse_nvrm_driver_version("GCC version: gcc 14.2.1\n"), None);
        assert_eq!(parse_nvrm_driver_version("NVRM version: unloaded\n"), None);
        assert_eq!(parse_nvrm_driver_version(""), None);
    }
}
