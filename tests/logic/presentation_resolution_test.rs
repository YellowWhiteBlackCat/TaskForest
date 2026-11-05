//! The cross-crate weld: `taskmanager-theme` owns every design number, and
//! `taskmanager-ui-contract::presentation` owns every role and fold. These
//! tests prove the two never drift, and that the four shapes resolve the SAME
//! measurement and motion intent through their own units/engines.

use taskmanager_theme::tokens::{
    DURATION_FAST, DURATION_HOVER, DURATION_MEDIUM, MotionPolicy, RowDensity, UiSize,
};
use taskmanager_ui_contract::FrontendShape;
use taskmanager_ui_contract::presentation::budget::{
    PerformancePageBudget, PerformanceSlotRequest,
};
use taskmanager_ui_contract::presentation::motion::{
    self, DurationClass, EasingClass, ForbiddenAnimation, MotionDurations, MotionFinding,
    MotionRole,
};
use taskmanager_ui_contract::presentation::resolution::{
    REPORTED_CONTROL_ROLE, ResolvedPerformance, ResolvedPresentation, parse_resolution_marker,
    resolution_marker_text, verify_resolved,
};
use taskmanager_ui_contract::presentation::sizes::{
    self, ControlRole, ControlState, DensityPoint, ResolutionUnit, SizeFinding, UiSizePoint,
};
use taskmanager_ui_contract::presentation::thresholds;
use taskmanager_ui_contract::presentation::vocabulary::{LayoutProfile, VerticalSpace};

/// The theme's UI-size axis as the contract consumes it: one lift per test
/// run, so a theme change moves the whole suite at once.
fn ui_point(ui_size: UiSize) -> UiSizePoint {
    UiSizePoint {
        control_height: ui_size.control_height().0,
        body_font_size: ui_size.body_font_size().0,
        renderer_scale: ui_size.renderer_scale(),
    }
}

/// The theme's row-density axis as the contract consumes it.
fn density_point(density: RowDensity) -> DensityPoint {
    DensityPoint {
        row_padding_y: density.row_padding_y().0,
        line_height: density.line_height().0,
    }
}

/// The theme's duration tokens as the contract consumes them.
fn theme_durations() -> MotionDurations {
    MotionDurations {
        fast: DURATION_FAST,
        hover: DURATION_HOVER,
        medium: DURATION_MEDIUM,
    }
}

#[test]
fn the_control_size_matrix_reads_the_theme_ladder_and_never_renumbers_it() {
    // There is ONE ladder: the theme's. The matrix resolves through it, so a
    // future theme change moves every shape at once and no consumer keeps a
    // second copy of the numbers.
    for ui_size in UiSize::ALL {
        let measured = sizes::resolve_size(
            FrontendShape::Gpui,
            ControlRole::Button,
            ControlState::Normal,
            ui_point(ui_size),
            density_point(RowDensity::Comfortable),
        );
        assert_eq!(measured.extent, ui_size.control_height().0);
    }
    for density in RowDensity::ALL {
        let measured = sizes::resolve_size(
            FrontendShape::Gpui,
            ControlRole::DataRow,
            ControlState::Normal,
            ui_point(UiSize::Standard),
            density_point(density),
        );
        assert_eq!(measured.padding_y, density.row_padding_y().0);
        assert_eq!(
            measured.extent,
            ui_size_row_extent(UiSize::Standard, density),
            "the row extent rides the theme's type size and line height"
        );
    }
}

/// The row extent the contract folds, computed from the theme's own numbers.
fn ui_size_row_extent(ui_size: UiSize, density: RowDensity) -> f32 {
    ui_size.body_font_size().0 * density.line_height().0 + density.row_padding_y().0 * 2.0
}

#[test]
fn every_shape_resolves_the_same_control_extent_in_its_own_unit() {
    for role in ControlRole::ALL {
        for state in ControlState::ALL {
            for ui_size in UiSize::ALL {
                for density in RowDensity::ALL {
                    let reference = sizes::resolve_size(
                        FrontendShape::Gpui,
                        role,
                        state,
                        ui_point(ui_size),
                        density_point(density),
                    );
                    for shape in FrontendShape::ALL {
                        let measured = sizes::resolve_size(
                            shape,
                            role,
                            state,
                            ui_point(ui_size),
                            density_point(density),
                        );
                        assert_eq!(measured.extent, reference.extent);
                        assert_eq!(measured.hit_target, reference.hit_target);
                        assert_eq!(measured.padding_x, reference.padding_x);
                        assert_eq!(measured.padding_y, reference.padding_y);
                        assert_eq!(
                            measured.unit,
                            ResolutionUnit::for_frontend(shape),
                            "{} resolves through its declared unit",
                            shape.name()
                        );
                        if ResolutionUnit::for_frontend(shape).supports_sub_unit_targets() {
                            assert_eq!(measured.realized_extent, reference.realized_extent);
                        } else {
                            assert_eq!(
                                measured.realized_extent,
                                measured.extent.ceil(),
                                "a terminal rounds the shared intent into whole cells"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn the_hit_target_floor_survives_every_ui_size_and_state() {
    for role in ControlRole::ALL {
        let floor = role.hit_target_floor();
        for ui_size in UiSize::ALL {
            for state in ControlState::ALL {
                let measured = sizes::resolve_size(
                    FrontendShape::Iced,
                    role,
                    state,
                    ui_point(ui_size),
                    density_point(RowDensity::Compact),
                );
                match floor {
                    None => assert_eq!(measured.hit_target, None),
                    Some(floor) => {
                        let target = measured
                            .hit_target
                            .expect("interactive roles always resolve a target");
                        assert!(
                            target >= floor * ui_size.renderer_scale(),
                            "{}:{}:{} target {target} fell below the scaled floor",
                            role.label(),
                            ui_size.control_height().0,
                            state as u8
                        );
                        assert!(target >= measured.extent);
                    }
                }
                if !state.changes_measurement() {
                    let normal = sizes::resolve_size(
                        FrontendShape::Iced,
                        role,
                        ControlState::Normal,
                        ui_point(ui_size),
                        density_point(RowDensity::Compact),
                    );
                    assert_eq!(measured.extent, normal.extent);
                }
            }
        }
    }
}

#[test]
fn density_never_shrinks_a_control_or_its_target() {
    for role in ControlRole::ALL {
        if role.admits_density() {
            continue;
        }
        for ui_size in UiSize::ALL {
            let comfortable = sizes::resolve_size(
                FrontendShape::Bevy,
                role,
                ControlState::Normal,
                ui_point(ui_size),
                density_point(RowDensity::Comfortable),
            );
            let compact = sizes::resolve_size(
                FrontendShape::Bevy,
                role,
                ControlState::Normal,
                ui_point(ui_size),
                density_point(RowDensity::Compact),
            );
            assert_eq!(comfortable.extent, compact.extent);
            assert_eq!(comfortable.hit_target, compact.hit_target);
        }
    }
}

#[test]
fn the_size_grid_has_no_findings_for_any_shape() {
    let ui_points: Vec<UiSizePoint> = UiSize::ALL.iter().map(|size| ui_point(*size)).collect();
    let density_points: Vec<DensityPoint> = RowDensity::ALL
        .iter()
        .map(|density| density_point(*density))
        .collect();
    for shape in FrontendShape::ALL {
        let findings: Vec<SizeFinding> = sizes::size_findings(shape, &ui_points, &density_points);
        assert!(
            findings.is_empty(),
            "{} size-matrix findings: {findings:?}",
            shape.name()
        );
    }
}

#[test]
fn the_motion_duration_slots_are_welded_to_the_theme_tokens() {
    let durations = theme_durations();
    assert_eq!(durations.for_class(DurationClass::Fast), DURATION_FAST);
    assert_eq!(durations.for_class(DurationClass::Hover), DURATION_HOVER);
    assert_eq!(durations.for_class(DurationClass::Medium), DURATION_MEDIUM);
}

#[test]
fn every_shape_resolves_the_same_motion_intent_with_its_own_engine() {
    let durations = theme_durations();
    for role in MotionRole::ALL {
        let duration =
            MotionPolicy::Normal.animation_duration(durations.for_class(role.duration_class()));
        let reference = motion::resolve_motion(FrontendShape::Gpui, role, duration);
        for shape in FrontendShape::ALL {
            let resolved = motion::resolve_motion(shape, role, duration);
            assert_eq!(resolved.duration_class, reference.duration_class);
            assert_eq!(resolved.easing, reference.easing);
            assert_eq!(resolved.duration, reference.duration);
            assert_eq!(resolved.role, role);
            assert_eq!(resolved.frontend, shape);
        }
        assert!(
            motion::binding(FrontendShape::Tui, role).is_some(),
            "TUI's terminal binding is explicit, not a silent omission"
        );
    }
}

#[test]
fn the_policy_seam_never_constructs_a_zero_duration_animation() {
    let durations = theme_durations();
    for role in MotionRole::ALL {
        let base = durations.for_class(role.duration_class());
        let no_motion = motion::resolve_motion(
            FrontendShape::Iced,
            role,
            MotionPolicy::NoMotion.animation_duration(base),
        );
        assert_eq!(no_motion.duration, None);
        let reduced = motion::resolve_motion(
            FrontendShape::Iced,
            role,
            MotionPolicy::Reduced.animation_duration(base),
        );
        assert_eq!(reduced.duration, Some(DURATION_FAST));
    }
}

#[test]
fn the_motion_grid_has_no_findings_and_the_forbidden_set_is_stable() {
    let findings: Vec<MotionFinding> = motion::motion_findings();
    assert!(findings.is_empty(), "motion findings: {findings:?}");
    assert_eq!(ForbiddenAnimation::ALL.len(), 4);
    assert_eq!(EasingClass::ALL.len(), 4);
    assert_eq!(DurationClass::ALL.len(), 3);
}

#[test]
fn a_resolution_record_round_trips_through_the_shared_marker_format() {
    // Each shape's record carries ITS OWN registered numbers, and each shape's
    // record verifies clean under that shape's bands. A record built from one
    // shape's geometry must NOT satisfy another shape's contract when the
    // registry says the bands differ.
    for shape in FrontendShape::ALL {
        let shape_bands = thresholds::bands(shape);
        let measured = sizes::resolve_size(
            shape,
            REPORTED_CONTROL_ROLE,
            ControlState::Normal,
            ui_point(UiSize::Standard),
            density_point(RowDensity::Comfortable),
        );
        let allocated = PerformancePageBudget::allocate(
            PerformanceSlotRequest {
                profile: LayoutProfile::Standard,
                workspace_width: 1_180.0,
                content_height: 748.0,
                page_padding: 16.0,
                sidebar_visible: true,
                persisted_sidebar_width: None,
            },
            &shape_bands,
        );
        let resolved = ResolvedPresentation {
            profile: LayoutProfile::Standard,
            vertical_space: VerticalSpace::Standard,
            page_padding: 16.0,
            width_input: 1_164.0,
            page_content_height: 748.0,
            control_height: UiSize::Standard.control_height().0,
            body_font_size: UiSize::Standard.body_font_size().0,
            renderer_scale: UiSize::Standard.renderer_scale(),
            row_padding_y: RowDensity::Comfortable.row_padding_y().0,
            line_height: RowDensity::Comfortable.line_height().0,
            control_extent: measured.extent,
            hit_target: measured.hit_target,
            performance: Some(ResolvedPerformance {
                runway: allocated.vertical,
                chart_inventory: allocated.chart_inventory,
                details: allocated.details,
                device_navigation: allocated.device_navigation,
                workspace_width: 1_180.0,
                content_height: 748.0,
                sidebar_visible: true,
                persisted_sidebar_width: None,
                sidebar_width: allocated.sidebar_width,
                stats_width: allocated.stats_width,
                main_width: allocated.main_width,
            }),
        };
        let text = resolution_marker_text(&resolved);
        let parsed = parse_resolution_marker(&text).expect("the record parses back");
        assert_eq!(parsed.profile, resolved.profile);
        assert_eq!(parsed.vertical_space, resolved.vertical_space);
        assert_eq!(parsed.control_height, resolved.control_height);
        assert_eq!(parsed.body_font_size, resolved.body_font_size);
        assert_eq!(parsed.row_padding_y, resolved.row_padding_y);
        assert_eq!(parsed.line_height, resolved.line_height);
        assert!((parsed.renderer_scale - resolved.renderer_scale).abs() <= 0.001);
        assert!((parsed.control_extent - resolved.control_extent).abs() <= 0.01);
        assert_eq!(
            parsed
                .hit_target
                .zip(resolved.hit_target)
                .map(|(left, right)| (left - right).abs() <= 0.01),
            Some(true),
            "the hit target must survive the transport"
        );
        let performance = parsed.performance.expect("the performance facts survive");
        let expected = resolved.performance.expect("present");
        assert_eq!(performance.runway, expected.runway);
        assert_eq!(performance.chart_inventory, expected.chart_inventory);
        assert_eq!(performance.details, expected.details);
        assert_eq!(performance.device_navigation, expected.device_navigation);
        let findings = verify_resolved(shape, &parsed, &shape_bands);
        assert!(
            findings.is_empty(),
            "{shape:?} round-tripped record must satisfy the contract: {findings:?}"
        );
    }
}
