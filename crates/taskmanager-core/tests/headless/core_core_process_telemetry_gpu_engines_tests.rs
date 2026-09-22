use super::*;
use crate::core::{DeviceStatus, FailureKind};

#[test]
fn empty_healthy_is_a_real_empty_not_unknown() {
    let breakdown = ProcessGpuEngines::empty_healthy(1_000);
    assert_eq!(breakdown.state, DeviceState::healthy(1_000));
    assert!(breakdown.engines.is_empty());
}

#[test]
fn unavailable_never_carries_engines() {
    let denied = DeviceState::healthy(10).transition(DeviceStatus::PermissionDenied, 20);
    let breakdown = ProcessGpuEngines::unavailable(denied);
    assert_eq!(breakdown.state.status, DeviceStatus::PermissionDenied);
    assert!(breakdown.engines.is_empty());
}

#[test]
fn cold_start_usage_is_unavailable_but_cumulative_is_observed() {
    let now_ms = 5_000;
    let engine = ProcessGpuEngineUsage {
        name: "render".into(),
        usage_pct: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        engine_time_ns: ScalarObservation::available(1_000_000, now_ms),
        engine_cycles: ScalarObservation::default(),
    };
    // The cumulative counter is honest from the first read ...
    assert_eq!(engine.engine_time_ns.current_value(), Some(&1_000_000));
    // ... but the rate is a typed gap until a second sample arrives.
    assert!(engine.usage_pct.current_value().is_none());
    // A cycles-only xe source keeps the ns counter unknown.
    assert!(engine.engine_cycles.current_value().is_none());
}

#[test]
fn cross_vendor_amd_and_intel_render_3d_parity() {
    // AMD: gfx, gfx0, gfx_0, 3d
    assert_eq!(
        ProcessGpuEngineClass::from_name("gfx"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("gfx0"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("gfx_0"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-engine-gfx"),
        ProcessGpuEngineClass::Render3D
    );

    // Intel: render, rcs, rcs0, 3d
    assert_eq!(
        ProcessGpuEngineClass::from_name("render"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("rcs"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("rcs0"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-engine-render"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-total-busy-render"),
        ProcessGpuEngineClass::Render3D
    );

    // Generic / D3D
    assert_eq!(
        ProcessGpuEngineClass::from_name("3D"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("Render/3D"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("Graphics (3D)"),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("render3d"),
        ProcessGpuEngineClass::Render3D
    );

    assert!(ProcessGpuEngineClass::Render3D.is_render_3d());
    assert!(ProcessGpuEngineClass::Render3D.is_3d());
    assert_eq!(ProcessGpuEngineClass::Render3D.as_str(), "render_3d");
    assert_eq!(ProcessGpuEngineClass::Render3D.display_label(), "Render/3D");
}

#[test]
fn cross_vendor_amd_and_intel_compute_parity() {
    // AMD: compute, compute0, comp
    assert_eq!(
        ProcessGpuEngineClass::from_name("compute"),
        ProcessGpuEngineClass::Compute
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("compute0"),
        ProcessGpuEngineClass::Compute
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("comp"),
        ProcessGpuEngineClass::Compute
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-engine-compute"),
        ProcessGpuEngineClass::Compute
    );

    // Intel: compute, ccs, ccs0
    assert_eq!(
        ProcessGpuEngineClass::from_name("ccs"),
        ProcessGpuEngineClass::Compute
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("ccs0"),
        ProcessGpuEngineClass::Compute
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("ccs1"),
        ProcessGpuEngineClass::Compute
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-total-busy-ccs0"),
        ProcessGpuEngineClass::Compute
    );

    assert!(ProcessGpuEngineClass::Compute.is_compute());
    assert_eq!(ProcessGpuEngineClass::Compute.as_str(), "compute");
    assert_eq!(ProcessGpuEngineClass::Compute.display_label(), "Compute");
}

#[test]
fn cross_vendor_amd_and_intel_copy_parity() {
    // AMD: sdma, sdma0, sdma1, dma
    assert_eq!(
        ProcessGpuEngineClass::from_name("sdma"),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("sdma0"),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("sdma1"),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("dma"),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-engine-sdma"),
        ProcessGpuEngineClass::Copy
    );

    // Intel: copy, bcs, bcs0, blitter
    assert_eq!(
        ProcessGpuEngineClass::from_name("copy"),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("bcs"),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("bcs0"),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("blitter"),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-engine-copy"),
        ProcessGpuEngineClass::Copy
    );

    // Generic
    assert_eq!(
        ProcessGpuEngineClass::from_name("Memory (Copy)"),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("transfer"),
        ProcessGpuEngineClass::Copy
    );

    assert!(ProcessGpuEngineClass::Copy.is_copy());
    assert_eq!(ProcessGpuEngineClass::Copy.as_str(), "copy");
    assert_eq!(ProcessGpuEngineClass::Copy.display_label(), "Copy");
}

#[test]
fn cross_vendor_amd_and_intel_video_decode_parity() {
    // AMD: dec, dec0, vcn_dec, vcn_dec0, uvd, uvd_dec, jpeg_dec
    assert_eq!(
        ProcessGpuEngineClass::from_name("dec"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("dec0"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("vcn_dec"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("vcn-dec"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("uvd"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("uvd_dec"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("jpeg_dec"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-engine-dec"),
        ProcessGpuEngineClass::VideoDecode
    );

    // Intel: video, vcs, vcs0, video-decode
    assert_eq!(
        ProcessGpuEngineClass::from_name("video"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("vcs"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("vcs0"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("video-decode"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-engine-video"),
        ProcessGpuEngineClass::VideoDecode
    );

    // Generic
    assert_eq!(
        ProcessGpuEngineClass::from_name("Video Decode"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("decode"),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("nvdec"),
        ProcessGpuEngineClass::VideoDecode
    );

    assert!(ProcessGpuEngineClass::VideoDecode.is_video_decode());
    assert!(ProcessGpuEngineClass::VideoDecode.is_video());
    assert_eq!(ProcessGpuEngineClass::VideoDecode.as_str(), "video_decode");
    assert_eq!(
        ProcessGpuEngineClass::VideoDecode.display_label(),
        "Video Decode"
    );
}

#[test]
fn cross_vendor_amd_and_intel_video_encode_parity() {
    // AMD: enc, enc0, vcn_enc, vcn_enc0, uvd_enc, vce, jpeg_enc
    assert_eq!(
        ProcessGpuEngineClass::from_name("enc"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("enc0"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("vcn_enc"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("vcn-enc"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("uvd_enc"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("vce"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("jpeg_enc"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-engine-enc"),
        ProcessGpuEngineClass::VideoEncode
    );

    // Intel: video-enhance, vecs, vecs0, video-encode
    assert_eq!(
        ProcessGpuEngineClass::from_name("video-enhance"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("video_enhance"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("vecs"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("vecs0"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("video-encode"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("drm-engine-video-enhance"),
        ProcessGpuEngineClass::VideoEncode
    );

    // Generic
    assert_eq!(
        ProcessGpuEngineClass::from_name("Video Encode"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("Video Processing"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("encode"),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("nvenc"),
        ProcessGpuEngineClass::VideoEncode
    );

    assert!(ProcessGpuEngineClass::VideoEncode.is_video_encode());
    assert!(ProcessGpuEngineClass::VideoEncode.is_video());
    assert_eq!(ProcessGpuEngineClass::VideoEncode.as_str(), "video_encode");
    assert_eq!(
        ProcessGpuEngineClass::VideoEncode.display_label(),
        "Video Encode"
    );
}

#[test]
fn unmapped_engine_names_remain_honest_unknown() {
    assert_eq!(
        ProcessGpuEngineClass::from_name(""),
        ProcessGpuEngineClass::Unknown
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("   "),
        ProcessGpuEngineClass::Unknown
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("unknown_engine"),
        ProcessGpuEngineClass::Unknown
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("npu"),
        ProcessGpuEngineClass::Unknown
    );
    assert_eq!(
        ProcessGpuEngineClass::from_name("dsp"),
        ProcessGpuEngineClass::Unknown
    );
    assert!(!ProcessGpuEngineClass::Unknown.is_classified());
    assert!(!ProcessGpuEngineClass::Other.is_classified());
}

#[test]
fn process_gpu_engine_usage_resolves_class() {
    let amd_engine = ProcessGpuEngineUsage {
        name: "gfx".into(),
        usage_pct: ScalarObservation::available(45.0, 100),
        engine_time_ns: ScalarObservation::available(10_000_000, 100),
        engine_cycles: ScalarObservation::default(),
    };
    assert_eq!(amd_engine.engine_class(), ProcessGpuEngineClass::Render3D);
    assert_eq!(amd_engine.class(), ProcessGpuEngineClass::Render3D);
    assert_eq!(amd_engine.kind(), ProcessGpuEngineClass::Render3D);

    let intel_engine = ProcessGpuEngineUsage::new(
        "video-enhance",
        ScalarObservation::available(10.0, 100),
        ScalarObservation::available(5_000_000, 100),
        ScalarObservation::default(),
    );
    assert_eq!(
        intel_engine.engine_class(),
        ProcessGpuEngineClass::VideoEncode
    );
}

#[test]
fn process_gpu_engines_lookup_by_class() {
    let engines = ProcessGpuEngines {
        state: DeviceState::healthy(1_000),
        engines: vec![
            ProcessGpuEngineUsage {
                name: "gfx".into(),
                usage_pct: ScalarObservation::available(30.0, 1_000),
                engine_time_ns: ScalarObservation::available(100_000, 1_000),
                engine_cycles: ScalarObservation::default(),
            },
            ProcessGpuEngineUsage {
                name: "sdma".into(),
                usage_pct: ScalarObservation::available(5.0, 1_000),
                engine_time_ns: ScalarObservation::available(20_000, 1_000),
                engine_cycles: ScalarObservation::default(),
            },
            ProcessGpuEngineUsage {
                name: "dec".into(),
                usage_pct: ScalarObservation::available(15.0, 1_000),
                engine_time_ns: ScalarObservation::available(50_000, 1_000),
                engine_cycles: ScalarObservation::default(),
            },
        ],
    };

    assert_eq!(
        engines
            .find_by_class(ProcessGpuEngineClass::Render3D)
            .map(|e| e.name.as_str()),
        Some("gfx")
    );
    assert_eq!(
        engines
            .find_by_class(ProcessGpuEngineClass::Copy)
            .map(|e| e.name.as_str()),
        Some("sdma")
    );
    assert_eq!(
        engines
            .find_by_class(ProcessGpuEngineClass::VideoDecode)
            .map(|e| e.name.as_str()),
        Some("dec")
    );
    assert!(
        engines
            .find_by_class(ProcessGpuEngineClass::Compute)
            .is_none()
    );
    assert_eq!(
        engines.filter_by_class(ProcessGpuEngineClass::Copy).count(),
        1
    );
}

#[test]
fn conversion_between_process_engine_class_and_gpu_engine_kind() {
    assert_eq!(
        GpuEngineKind::from(ProcessGpuEngineClass::Render3D),
        GpuEngineKind::Render
    );
    assert_eq!(
        GpuEngineKind::from(ProcessGpuEngineClass::Compute),
        GpuEngineKind::Compute
    );
    assert_eq!(
        GpuEngineKind::from(ProcessGpuEngineClass::Copy),
        GpuEngineKind::Copy
    );
    assert_eq!(
        GpuEngineKind::from(ProcessGpuEngineClass::VideoDecode),
        GpuEngineKind::VideoDecode
    );
    assert_eq!(
        GpuEngineKind::from(ProcessGpuEngineClass::VideoEncode),
        GpuEngineKind::VideoEncode
    );
    assert_eq!(
        GpuEngineKind::from(ProcessGpuEngineClass::Unknown),
        GpuEngineKind::Unknown
    );

    assert_eq!(
        ProcessGpuEngineClass::from(GpuEngineKind::Render),
        ProcessGpuEngineClass::Render3D
    );
    assert_eq!(
        ProcessGpuEngineClass::from(GpuEngineKind::Compute),
        ProcessGpuEngineClass::Compute
    );
    assert_eq!(
        ProcessGpuEngineClass::from(GpuEngineKind::Copy),
        ProcessGpuEngineClass::Copy
    );
    assert_eq!(
        ProcessGpuEngineClass::from(GpuEngineKind::VideoDecode),
        ProcessGpuEngineClass::VideoDecode
    );
    assert_eq!(
        ProcessGpuEngineClass::from(GpuEngineKind::VideoEncode),
        ProcessGpuEngineClass::VideoEncode
    );
    assert_eq!(
        ProcessGpuEngineClass::from(GpuEngineKind::Unknown),
        ProcessGpuEngineClass::Unknown
    );
}

#[test]
fn serde_roundtrip_process_gpu_engine_class() {
    for class in ProcessGpuEngineClass::ALL {
        let json = serde_json::to_string(class).expect("serialize");
        let parsed: ProcessGpuEngineClass = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*class, parsed);
    }
}
