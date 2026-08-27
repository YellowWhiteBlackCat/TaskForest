// test-intent: behavior
//! Disk page split-series chart wiring: the two-series graph spec is built
//! from pure helpers (legend series construction, the unit-pair formatter),
//! so the label/samples/color contract and the formatter's agreement with the
//! shared `summary_value` authority are pinned headlessly — the same rules the
//! canvas renders.

use std::rc::Rc;

use super::*;

/// A series carries its own window and legend label, with the placeholder
/// white the factory contract overwrites from the family token.
#[test]
fn disk_split_series_carries_label_samples_and_placeholder_color() {
    let series = disk_split_series("Read".to_string(), Rc::from([1.0, 2.5, 4.0].as_slice()));
    assert_eq!(series.label, "Read");
    assert_eq!(series.samples.as_ref(), [1.0, 2.5, 4.0]);
    assert_eq!(series.color, iced::Color::WHITE);
}

/// The injected formatter resolves through the shared `throughput_scale` /
/// `summary_value` authority for every drive unit pair, so the two-series
/// graph's ticks and hover pill never disagree with the scalar rows — and a
/// non-finite gap stays an honest dash in every unit family.
#[test]
fn drive_throughput_formatter_matches_the_summary_value_authority() {
    let pairs = [
        UnitPrefs {
            use_bytes: true,
            use_base2: true,
        },
        UnitPrefs {
            use_bytes: true,
            use_base2: false,
        },
        UnitPrefs {
            use_bytes: false,
            use_base2: true,
        },
        UnitPrefs {
            use_bytes: false,
            use_base2: false,
        },
    ];
    for units in pairs {
        let format = drive_throughput_formatter(units);
        for value in [0.0_f32, 1_500_000.0, 1_048_576.0] {
            assert_eq!(
                format(value),
                device_chart::summary_value(throughput_scale(units), value),
                "formatter drift for {units:?} at {value}"
            );
        }
        assert_eq!(format(f32::NAN), "—", "a gap stays a dash in {units:?}");
    }
}
