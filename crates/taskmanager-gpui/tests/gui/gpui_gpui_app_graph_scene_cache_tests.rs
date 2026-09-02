//! The store's lookup keying, decimation, and rate-gate rules are pure
//! enough to test without a window; the paint replay itself is exercised
//! end-to-end by the page-specific render-path suites.
use super::super::cache::GraphPresentationCache;
use super::{
    BadgeDirections, GraphSceneKey, GraphStaticSceneKey, MAX_GRAPH_SCENE_ENTRIES,
    MAX_GRAPH_STATIC_SCENE_ENTRIES, MAX_SPARK_SCENE_ENTRIES, MIN_HOVER_REFRESH_INTERVAL,
    SeriesSlot, SparkSceneKey, compose_badge_text, decimate_run, hover_refresh_is_due, rgba_bits,
};
use crate::gpui_app::graph::GraphOpts;
use gpui::{Bounds, Point, Rgba, Size, px};
use std::rc::Rc;
use std::time::{Duration, Instant};

fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<gpui::Pixels> {
    Bounds {
        origin: Point::new(px(x), px(y)),
        size: Size::new(px(w), px(h)),
    }
}

fn base() -> Rgba {
    Rgba {
        r: 0.2,
        g: 0.6,
        b: 0.9,
        a: 1.0,
    }
}

/// Test-local canonical single-series key (primary slot): the production
/// `#[cfg(test)]` constructors were removed from production source per the
/// test-layout guard, so the keying tests build keys through the same
/// `for_series` entry the single-series paint path uses.
fn key(
    samples: &Rc<[f32]>,
    bounds: Bounds<gpui::Pixels>,
    base: Rgba,
    opts: GraphOpts,
) -> GraphSceneKey {
    GraphSceneKey::for_series(samples, bounds, base, opts, SeriesSlot::Primary)
}

/// Test-local static projection; a child module reads the parent's private
/// key fields directly, keeping the production type free of test markers.
trait StaticKeyExt {
    fn static_key(&self) -> GraphStaticSceneKey;
}

impl StaticKeyExt for GraphSceneKey {
    fn static_key(&self) -> GraphStaticSceneKey {
        GraphStaticSceneKey {
            origin: self.origin,
            size: self.size,
            theme_key: self.theme_key,
            fill_alpha_bits: self.fill_alpha_bits,
            grid_alpha_bits: self.grid_alpha_bits,
            hlines: self.hlines,
            vlines: self.vlines,
            stroke_width_bits: self.stroke_width_bits,
            gradient_fill: self.gradient_fill,
            ref_lines: self.ref_lines,
        }
    }
}

/// The key must distinguish every geometry-relevant input: moving the
/// canvas, resizing it, a different samples allocation, and each opt-in
/// knob. One canonical "source" key is varied field by field.
#[test]
fn graph_scene_key_tracks_every_geometry_input() {
    let samples: Rc<[f32]> = Rc::from([1.0, 2.0, 3.0].as_slice());
    let other_samples: Rc<[f32]> = Rc::from([1.0, 2.0, 3.0].as_slice());
    let opts = GraphOpts::default();
    let source = key(&samples, bounds(10.0, 20.0, 300.0, 150.0), base(), opts);

    macro_rules! changed {
        ($name:ident, $mutation:expr) => {
            assert_ne!(source, $mutation, stringify!($name));
        };
    }

    changed!(
        moved,
        key(&samples, bounds(11.0, 20.0, 300.0, 150.0), base(), opts)
    );
    changed!(
        resized,
        key(&samples, bounds(10.0, 20.0, 301.0, 150.0), base(), opts)
    );
    changed!(
        reallocated_samples,
        key(
            &other_samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            opts
        )
    );
    changed!(
        different_base,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            Rgba { r: 0.3, ..base() },
            opts
        )
    );
    changed!(
        different_max,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts { max: 250.0, ..opts }
        )
    );
    changed!(
        different_fill_alpha,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts {
                fill_alpha: 0.25,
                ..opts
            }
        )
    );
    changed!(
        different_grid_alpha,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts {
                grid_alpha: 0.2,
                ..opts
            }
        )
    );
    changed!(
        different_vertical_grid,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts { vlines: 4, ..opts }
        )
    );
    changed!(
        gradient_on,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts {
                gradient_fill: true,
                ..opts
            }
        )
    );
    changed!(
        different_stroke,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts {
                stroke_width: 2.0,
                ..opts
            }
        )
    );
    changed!(
        different_grid,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts { hlines: 4, ..opts }
        )
    );
    changed!(
        different_window,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts {
                data_points: 120,
                ..opts
            }
        )
    );
    changed!(
        ref_lines_on,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts {
                ref_lines: true,
                ..opts
            }
        )
    );
    changed!(
        smoothing_off,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts {
                smooth: false,
                ..opts
            }
        )
    );

    changed!(
        different_revision,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts {
                animation_epoch: opts.animation_epoch + 1,
                ..opts
            }
        )
    );

    // Without an extra historical sample there is nothing to slide, so the
    // presentation flag alone does not change the cached paths.
    let equivalent = key(
        &samples,
        bounds(10.0, 20.0, 300.0, 150.0),
        base(),
        GraphOpts {
            sliding: !opts.sliding,
            ..opts
        },
    );
    assert_eq!(source, equivalent);

    // With one extra sample the slide base uses a different x mapping, so the
    // sliding and settled scenes must be cached separately.
    let long_samples: Rc<[f32]> = Rc::from((0..601).map(|i| i as f32).collect::<Vec<_>>());
    let slide_key = key(
        &long_samples,
        bounds(10.0, 20.0, 300.0, 150.0),
        base(),
        GraphOpts {
            sliding: true,
            ..opts
        },
    );
    let settled_key = key(
        &long_samples,
        bounds(10.0, 20.0, 300.0, 150.0),
        base(),
        GraphOpts {
            sliding: false,
            ..opts
        },
    );
    assert_ne!(slide_key, settled_key);

    // A revision, scale, or data-window change belongs only to the dynamic
    // path. Static grid/fill geometry can stay resident while those inputs
    // change; size and theme remain hard invalidation boundaries.
    let static_source = source.static_key();
    assert_eq!(
        static_source,
        GraphStaticSceneKey {
            fill_alpha_bits: opts.fill_alpha.to_bits(),
            grid_alpha_bits: opts.grid_alpha.to_bits(),
            hlines: opts.hlines,
            vlines: opts.vlines,
            stroke_width_bits: opts.stroke_width.to_bits(),
            gradient_fill: opts.gradient_fill,
            ref_lines: opts.ref_lines,
            ..static_source
        }
    );
    assert_eq!(
        static_source,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            base(),
            GraphOpts {
                max: 250.0,
                data_points: 120,
                animation_epoch: opts.animation_epoch + 1,
                ..opts
            },
        )
        .static_key()
    );
    assert_ne!(
        static_source,
        key(
            &samples,
            bounds(10.0, 20.0, 300.0, 150.0),
            Rgba { r: 0.3, ..base() },
            opts,
        )
        .static_key()
    );
}

/// A longer series with identical window-space inputs must not collide
/// with a reallocated shorter one just because the address matches: the
/// length is part of the key.
#[test]
fn spark_key_separates_reused_addresses_by_length() {
    let short: Rc<[f32]> = Rc::from([1.0].as_slice());
    let key = SparkSceneKey {
        samples_addr: Rc::as_ptr(&short).addr(),
        samples_len: short.len(),
        origin: (0.0, 0.0),
        size: (48.0, 16.0),
        color_bits: rgba_bits(base()),
    };
    let same_address_longer = SparkSceneKey {
        samples_len: short.len() + 1,
        ..key
    };
    assert_ne!(key, same_address_longer);
}

/// The interval rule: no previous refresh ⇒ due; within the interval ⇒
/// not due; at or past the interval ⇒ due again.
#[test]
fn hover_refresh_gate_interval_rule() {
    let now = Instant::now();
    assert!(hover_refresh_is_due(None, now, MIN_HOVER_REFRESH_INTERVAL));
    assert!(!hover_refresh_is_due(
        Some(now),
        now,
        MIN_HOVER_REFRESH_INTERVAL
    ));
    let earlier = now - MIN_HOVER_REFRESH_INTERVAL;
    assert!(hover_refresh_is_due(
        Some(earlier),
        now,
        MIN_HOVER_REFRESH_INTERVAL
    ));
    let just_before = now - (MIN_HOVER_REFRESH_INTERVAL - Duration::from_millis(1));
    assert!(!hover_refresh_is_due(
        Some(just_before),
        now,
        MIN_HOVER_REFRESH_INTERVAL
    ));
    let future = now + MIN_HOVER_REFRESH_INTERVAL;
    assert!(!hover_refresh_is_due(
        Some(future),
        now,
        MIN_HOVER_REFRESH_INTERVAL
    ));
}

/// A busy pointer in one window must not consume another window's first
/// hover refresh. The gate is presentation lifecycle state, not a process-wide
/// singleton.
#[test]
fn hover_refresh_gate_is_scoped_to_its_window() {
    let now = Instant::now();
    let mut first_window = GraphPresentationCache::default();
    let mut second_window = GraphPresentationCache::default();

    assert!(first_window.hover_refresh_due(now));
    assert!(!first_window.hover_refresh_due(now));
    assert!(
        second_window.hover_refresh_due(now),
        "another window owns an independent first-refresh allowance"
    );

    first_window.reset_hover_refresh();
    second_window.reset_hover_refresh();
}

/// The capacity caps are part of the memory contract documented in
/// `docs/ARCH.md`; pin them so a silent change of either
/// is a review-level event.
#[test]
fn scene_store_caps_stay_at_the_documented_values() {
    assert_eq!(MAX_GRAPH_SCENE_ENTRIES, 160);
    assert_eq!(MAX_GRAPH_STATIC_SCENE_ENTRIES, 64);
    assert_eq!(MAX_SPARK_SCENE_ENTRIES, 256);
}

/// LTTB keeps the endpoints, preserves strictly increasing original
/// indices (the time axis never distorts), respects the budget, and
/// retains an interior spike that straight bucket-averaging would erase.
#[test]
fn lttb_decimation_preserves_endpoints_indices_and_spikes() {
    // 100 samples: flat line with one spike at index 37.
    let run: Vec<(usize, f32)> = (0..100)
        .map(|index| (index, if index == 37 { 99.0 } else { 1.0 }))
        .collect();
    let decimated = decimate_run(&run, 10);
    assert_eq!(decimated.len(), 10, "budget is respected");
    assert_eq!(decimated.first(), Some(&(0, 1.0)), "oldest sample kept");
    assert_eq!(
        decimated.last(),
        Some(&(99, 1.0)),
        "newest sample kept (right-anchored fidelity)"
    );
    let indices: Vec<usize> = decimated.iter().map(|(index, _)| *index).collect();
    let mut sorted = indices.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(indices, sorted, "indices stay strictly increasing");
    assert!(
        decimated
            .iter()
            .any(|&(index, sample)| index == 37 && sample == 99.0),
        "the spike survives decimation"
    );

    // A run within the budget comes back unchanged.
    let short: Vec<(usize, f32)> = (0..5).map(|index| (index, index as f32)).collect();
    assert_eq!(decimate_run(&short, 10), short);
    // Degenerate budgets fall back to the original run rather than
    // producing a two-point line that erases everything.
    assert_eq!(decimate_run(&short, 2), short);
    assert_eq!(decimate_run(&short, 0), short);
}

/// LTTB on a monotonic ramp keeps a strictly monotonic selection: the
/// visual trend direction can never invert across decimation.
#[test]
fn lttb_decimation_never_inverts_a_monotonic_trend() {
    let run: Vec<(usize, f32)> = (0..60).map(|index| (index, index as f32)).collect();
    let decimated = decimate_run(&run, 12);
    for pair in decimated.windows(2) {
        assert!(pair[0].1 < pair[1].1, "trend must stay increasing");
    }
}

/// Parity with the neutral kernel: the end adapter's pair output must be
/// exactly the neutral function's selected positions mapped back through
/// the run, across fixed inputs covering a spike on a flat baseline,
/// equal values, a monotonic trend, and gap-adjacent sparse original
/// indices — for identity, degenerate, and decimating budgets.
#[test]
fn decimate_run_parity_with_the_neutral_lttb_kernel() {
    let cases: Vec<Vec<(usize, f32)>> = vec![
        (0..100)
            .map(|index| (index, if index == 37 { 99.0 } else { 1.0 }))
            .collect(),
        (0..40).map(|index| (index, 7.5)).collect(),
        (0..60).map(|index| (index, index as f32)).collect(),
        vec![
            (0, 1.0),
            (1, 2.0),
            (2, 1.0),
            (50, 9.0),
            (51, 3.0),
            (52, 1.0),
        ],
    ];
    for run in cases {
        for budget in [0, 1, 2, 3, 7, 12, run.len(), run.len() + 5] {
            let expected: Vec<(usize, f32)> =
                taskmanager_application::history_decimation::lttb_indices(&run, budget)
                    .into_iter()
                    .map(|position| run[position])
                    .collect();
            assert_eq!(
                decimate_run(&run, budget),
                expected,
                "parity must hold for len {} budget {budget}",
                run.len()
            );
        }
    }
}

// test-intent: behavior
/// The two series slots of a dual-series graph must own separate dynamic
/// scenes even when both directions share one allocation identity, one color,
/// and one window: the slot is part of the dynamic key, so the under-painted
/// secondary can never replay the primary's entry (or vice versa). The static
/// grid/fill key stays slot-free — both series share one grid scene.
#[test]
fn graph_dynamic_scene_key_separates_the_two_series_slots() {
    let samples: Rc<[f32]> = Rc::from([1.0, 2.0, 3.0].as_slice());
    let opts = GraphOpts::default();
    let canvas = bounds(10.0, 20.0, 300.0, 150.0);
    let primary = GraphSceneKey::for_series(&samples, canvas, base(), opts, SeriesSlot::Primary);
    let secondary =
        GraphSceneKey::for_series(&samples, canvas, base(), opts, SeriesSlot::Secondary);
    assert_ne!(
        primary, secondary,
        "the full key separates the two series slots"
    );
    assert_ne!(
        primary.dynamic_key(),
        secondary.dynamic_key(),
        "the dynamic store keys separate the two series slots"
    );
    assert_eq!(
        primary.static_key(),
        secondary.static_key(),
        "the static grid/fill scene is shared by both slots"
    );
    // The canonical single-series constructor stays the primary slot.
    assert_eq!(key(&samples, canvas, base(), opts), primary);
}

// test-intent: behavior
/// The dual-series value badge must speak for BOTH directions (a read/write
/// graph that shows only the read rate is lying by omission), each direction
/// labeled through the same pairing the legend names, with a gap direction
/// staying silent instead of fabricating a value — mirroring iced's
/// `readout_text` composition.
#[test]
fn badge_text_composes_both_directions_with_labels() {
    let read_write = BadgeDirections {
        primary_label: "Read",
        secondary_samples: &[1.0, f32::NAN, 340.0],
        secondary_label: "Write",
    };
    assert_eq!(
        compose_badge_text(
            Some(1.2),
            Some(&read_write),
            Some(|v| format!("{v:.1} MB/s"))
        ),
        Some("Read 1.2 MB/s · Write 340.0 MB/s".to_owned())
    );
    // The secondary's newest sample is the only gap: the pill degrades to
    // the primary direction alone rather than showing a fabricated write.
    let write_gap = BadgeDirections {
        primary_label: "Read",
        secondary_samples: &[1.0, 2.0, f32::NAN],
        secondary_label: "Write",
    };
    assert_eq!(
        compose_badge_text(
            Some(1.2),
            Some(&write_gap),
            Some(|v| format!("{v:.1} MB/s"))
        ),
        Some("Read 1.2 MB/s".to_owned())
    );
    // The primary's newest sample is the gap: the secondary's evidence still
    // reads out, labeled, so a live direction is never hidden by the other
    // direction's provider gap.
    let read_gap = BadgeDirections {
        primary_label: "Read",
        secondary_samples: &[1.0, 2.0, 340.0],
        secondary_label: "Write",
    };
    assert_eq!(
        compose_badge_text(None, Some(&read_gap), Some(|v| format!("{v:.1} MB/s"))),
        Some("Write 340.0 MB/s".to_owned())
    );
    // Neither direction has a finite newest sample: no pill at all.
    let both_gap = BadgeDirections {
        primary_label: "Read",
        secondary_samples: &[f32::NAN],
        secondary_label: "Write",
    };
    assert_eq!(
        compose_badge_text(None, Some(&both_gap), Some(|v| format!("{v:.1}"))),
        None
    );
    // The no-formatter fallback keeps the plain `{:.1}` rendering on the
    // composed pill.
    assert_eq!(
        compose_badge_text(Some(2.5), Some(&read_write), None),
        Some("Read 2.5 · Write 340.0".to_owned())
    );
}

// test-intent: behavior
/// The single-series pill keeps its exact legacy readout: the newest value
/// through the caller's formatter (or the `{:.1}` fallback), and no pill when
/// the newest sample is a gap.
#[test]
fn badge_text_single_series_readout_is_unchanged() {
    let pct = |v: f32| format!("{v:.0}%");
    assert_eq!(
        compose_badge_text(Some(43.25), None, Some(pct)),
        Some("43%".to_owned())
    );
    assert_eq!(
        compose_badge_text(Some(43.25), None, None),
        Some("43.2".to_owned())
    );
    assert_eq!(compose_badge_text(None, None, Some(pct)), None);
}
