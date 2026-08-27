use super::*;

#[test]
fn class_keyword_covers_the_five_classes() {
    assert_eq!(class_keyword(CLASS_RENDER), "render");
    assert_eq!(class_keyword(CLASS_COPY), "copy");
    assert_eq!(class_keyword(CLASS_VIDEO), "video");
    assert_eq!(class_keyword(CLASS_VIDEO_ENHANCE), "video-enhance");
    assert_eq!(class_keyword(CLASS_COMPUTE), "compute");
    assert_eq!(class_keyword(99), "unknown");
}

#[test]
fn engine_label_maps_known_and_unknown_names() {
    // xe per-class names.
    assert_eq!(engine_label("render"), "Render/3D");
    assert_eq!(engine_label("copy"), "Copy");
    assert_eq!(engine_label("compute"), "Compute");
    assert_eq!(engine_label("video"), "Video Decode");
    assert_eq!(engine_label("video-enhance"), "Video Encode");
    // i915 per-instance names.
    assert_eq!(engine_label("rcs0"), "Render/3D");
    assert_eq!(engine_label("bcs1"), "Copy");
    assert_eq!(engine_label("ccs2"), "Compute");
    assert_eq!(engine_label("vcs0"), "Video Decode");
    assert_eq!(engine_label("vecs0"), "Video Encode");
    // Unknown future engine: passed through, upper-cased, separators→spaces.
    assert_eq!(engine_label("matrix_unit"), "MATRIX UNIT");
    assert_eq!(engine_label("future-class"), "FUTURE CLASS");
}

#[test]
fn parse_i915_engine_handles_instance_and_long_form_names() {
    let rcs0 = parse_i915_engine("rcs0").expect("rcs0");
    assert_eq!(rcs0.class, CLASS_RENDER);
    assert_eq!(rcs0.instance, 0);

    let bcs1 = parse_i915_engine("bcs1").expect("bcs1");
    assert_eq!(bcs1.class, CLASS_COPY);
    assert_eq!(bcs1.instance, 1);

    assert_eq!(
        parse_i915_engine("vecs0").map(|parsed| parsed.class),
        Some(CLASS_VIDEO_ENHANCE)
    );
    assert_eq!(
        parse_i915_engine("vcs2").map(|parsed| (parsed.class, parsed.instance)),
        Some((CLASS_VIDEO, 2))
    );

    // Long-form → instance 0.
    assert_eq!(
        parse_i915_engine("render").map(|parsed| (parsed.class, parsed.instance)),
        Some((CLASS_RENDER, 0))
    );

    // Unknown / bare mnemonic / non-digit → None (no fabrication).
    assert!(parse_i915_engine("rcs").is_none());
    assert!(parse_i915_engine("rcsX").is_none());
    assert!(parse_i915_engine("future_unit").is_none());
}

#[test]
fn parse_xe_engine_accepts_bare_mnemonics_and_long_forms() {
    // Bare mnemonics (the on-box Core Ultra layout): instance 0.
    let rcs = parse_xe_engine("rcs").expect("bare rcs");
    assert_eq!(rcs.class, CLASS_RENDER);
    assert_eq!(rcs.instance, 0);

    assert_eq!(parse_xe_engine("bcs").map(|p| p.class), Some(CLASS_COPY));
    assert_eq!(parse_xe_engine("ccs").map(|p| p.class), Some(CLASS_COMPUTE));
    // vecs before vcs: encode must not collapse into decode.
    assert_eq!(
        parse_xe_engine("vecs").map(|p| p.class),
        Some(CLASS_VIDEO_ENHANCE)
    );
    assert_eq!(parse_xe_engine("vcs").map(|p| p.class), Some(CLASS_VIDEO));

    // Long-form names, instance 0.
    assert_eq!(
        parse_xe_engine("render").map(|p| (p.class, p.instance)),
        Some((CLASS_RENDER, 0))
    );
    assert_eq!(
        parse_xe_engine("video-enhance").map(|p| p.class),
        Some(CLASS_VIDEO_ENHANCE)
    );

    // Optional digit tail tolerated; garbage rejected.
    assert_eq!(
        parse_xe_engine("rcs0").map(|p| (p.class, p.instance)),
        Some((CLASS_RENDER, 0))
    );
    assert!(parse_xe_engine("rcsX").is_none());
    assert!(parse_xe_engine("future_unit").is_none());
    assert!(parse_xe_engine(".defaults").is_none());
}
