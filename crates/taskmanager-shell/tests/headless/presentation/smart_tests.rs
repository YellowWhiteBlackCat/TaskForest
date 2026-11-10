use super::*;
use taskforest_core::core::metrics::DiskMetrics;

fn one_disk() -> DiskMetrics {
    DiskMetrics::new("nvme0n1")
}

/// The endurance readout is one composition: the marker rides the number only
/// when the device reached its rated life, and the sentence form is the same
/// reading with the catalog note appended.
#[test]
fn endurance_readout_wears_the_marker_only_past_the_rated_life() {
    assert_eq!(endurance_used_readout(Some(99.0), None), "99%");
    assert_eq!(endurance_used_readout(Some(100.0), None), "100% \u{26a0}");
    assert_eq!(endurance_used_readout(Some(104.0), None), "104% \u{26a0}");
    assert_eq!(
        endurance_used_readout(Some(104.0), Some("(exceeded rated life)")),
        "104% \u{26a0} (exceeded rated life)",
        "a surface with room for a sentence says why, on the same reading"
    );
}

/// Absence never becomes a critical reading. `is_nvme_wear_critical` answers
/// `true` for infinity, so an unfiltered gate would paint an alert marker
/// beside the shared dash and claim a fact the host never measured.
#[test]
fn an_unusable_endurance_reading_wears_no_marker() {
    assert_eq!(endurance_used_readout(None, None), "\u{2014}");
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            endurance_used_readout(Some(value), None),
            "\u{2014}",
            "{value} is not a reading, so it cannot be a critical one"
        );
        assert_eq!(
            endurance_used_readout(Some(value), Some("note")),
            "\u{2014}"
        );
    }
}

/// The spare readout resolves the threshold through core and never re-derives
/// it: the controller's own value wins, the default fills its absence, and the
/// compact form carries the bare symbol clause.
#[test]
fn spare_readout_reports_the_observed_threshold() {
    let mut disk = one_disk();
    disk.smart_available_spare_pct = Some(4.0);
    assert_eq!(spare_pool_readout(&disk, None), "4% \u{26a0} (\u{2264}10%)");
    assert_eq!(
        spare_pool_readout(&disk, Some("(warning \u{2264} {threshold})")),
        "4% \u{26a0} (warning \u{2264} 10%)",
        "the clause slot is filled with the same spelling as the compact form"
    );

    disk.smart_available_spare_threshold_pct = Some(25.0);
    assert_eq!(
        spare_pool_readout(&disk, None),
        "4% \u{26a0} (\u{2264}25%)",
        "a controller-reported threshold outranks the default"
    );

    disk.smart_available_spare_pct = Some(30.0);
    assert_eq!(spare_pool_readout(&disk, None), "30%");
    disk.smart_available_spare_pct = Some(25.0);
    assert_eq!(
        spare_pool_readout(&disk, None),
        "25% \u{26a0} (\u{2264}25%)",
        "the edge belongs to the warning"
    );
}

/// A pool that was never read is not a warning, and a broken threshold cannot
/// invent one.
#[test]
fn an_unobserved_or_unusable_spare_pool_judges_nothing() {
    let healthy = one_disk();
    assert_eq!(spare_pool_readout(&healthy, None), "\u{2014}");

    let mut observed = one_disk();
    observed.smart_available_spare_pct = Some(2.0);
    for threshold in [f32::NAN, f32::INFINITY] {
        observed.smart_available_spare_threshold_pct = Some(threshold);
        assert_eq!(
            spare_pool_readout(&observed, None),
            "2%",
            "{threshold} is not a usable threshold, so no warning may rise"
        );
    }

    observed.smart_available_spare_pct = Some(f32::NAN);
    observed.smart_available_spare_threshold_pct = None;
    assert_eq!(spare_pool_readout(&observed, None), "\u{2014}");
}

/// The temperature label is the localized word plus the marker; the marker is
/// never baked into the word, so every language reads its own label.
#[test]
fn the_temperature_label_carries_the_marker_without_owning_the_word() {
    assert_eq!(smart_temperature_label("Temperature", false), "Temperature");
    assert_eq!(
        smart_temperature_label("\u{6e29}\u{5ea6}", false),
        "\u{6e29}\u{5ea6}"
    );
    assert_eq!(
        smart_temperature_label("\u{6e29}\u{5ea6}", true),
        "\u{6e29}\u{5ea6} \u{26a0}"
    );
    assert_eq!(
        smart_temperature_label("Temperature", true),
        "Temperature \u{26a0}",
        "one space between the word and the marker, whatever the language"
    );
}

/// The reference spellings the three text frontends wrote for themselves, kept
/// here as the thing the owner must answer like. The GPUI / Iced / TUI compact
/// rows spelled the percent through a helper and appended the marker at the
/// edge; Iced spelled the percent with the bare `percent_text` while GPUI and
/// TUI used the absence-aware readout, so a measured value has to come out
/// byte-identically for all three. Only a non-finite reading may differ, and
/// there the owner is the honest side: a dash instead of `inf%`.
mod former_spellings {
    use taskforest_core::core::percent::percent_text;
    use taskforest_core::core::storage_health::is_nvme_wear_critical;

    /// The percent half of the endurance composition the compact rows shared.
    pub fn endurance_compact_old(percent_used: Option<f32>, dash: &str) -> String {
        match percent_used {
            Some(value) => percent_text(value),
            None => dash.to_owned(),
        }
    }

    /// The spare-pool composition each compact row spelled for itself:
    /// `percent_text` on both sides of the marker, inclusive at the edge.
    pub fn spare_compact_old(
        spare: Option<f32>,
        threshold: f32,
        needs: bool,
        dash: &str,
    ) -> String {
        let Some(spare) = spare else {
            return dash.to_owned();
        };
        if needs {
            format!(
                "{} \u{26a0} (\u{2264}{})",
                percent_text(spare),
                percent_text(threshold)
            )
        } else {
            percent_text(spare)
        }
    }

    pub fn wear_old(percent_used: f32) -> bool {
        is_nvme_wear_critical(percent_used)
    }
}

/// Over every value a provider can report, the owner answers exactly what the
/// deleted per-frontend copies answered, so folding them cannot move a pixel.
#[test]
fn the_owner_answers_like_the_spellings_it_replaced() {
    use former_spellings::{endurance_compact_old, spare_compact_old, wear_old};
    let dash = "\u{2014}";

    for i in 0..=400 {
        let value = i as f32 / 4.0; // 0.0 ..= 100.0 in quarters
        for note in [None, Some("(exceeded rated life)")] {
            let plain = endurance_compact_old(Some(value), dash);
            let expected = if wear_old(value) {
                match note {
                    Some(note) => format!("{plain} \u{26a0} {note}"),
                    None => format!("{plain} \u{26a0}"),
                }
            } else {
                plain
            };
            assert_eq!(
                endurance_used_readout(Some(value), note),
                expected,
                "endurance {value} with note {note:?}"
            );
        }

        let mut disk = one_disk();
        for spare_value in [Some(value), Some(value + 200.0), None] {
            disk.smart_available_spare_pct = spare_value;
            for threshold in [10.0, 25.0, value] {
                disk.smart_available_spare_threshold_pct = Some(threshold);
                let needs = disk.smart_spare_pool_needs_warning();
                assert_eq!(
                    spare_pool_readout(&disk, None),
                    spare_compact_old(spare_value, threshold, needs, dash),
                    "spare {spare_value:?} threshold {threshold}"
                );
            }
        }
    }
}

/// The one place the owner deliberately disagrees with the copies it replaced:
/// a positive infinity endurance is not a reading, yet `is_nvme_wear_critical`
/// answers `true` for it, so the old bare gate would have hung a critical alert
/// beside the shared dash. Absence never wears a marker.
#[test]
fn a_non_finite_endurance_reading_wears_no_marker() {
    assert!(
        former_spellings::wear_old(f32::INFINITY),
        "the replaced gate called positive infinity critical wear"
    );
    assert!(
        !former_spellings::wear_old(f32::NAN),
        "NaN was already judged not-critical by the replaced gate"
    );
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            endurance_used_readout(Some(value), None),
            "\u{2014}",
            "{value}: absence is reported, never an alert on an unmeasured device"
        );
    }
}
