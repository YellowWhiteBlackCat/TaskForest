use super::{lttb_indices, stride_envelope, stride_envelope_positions};

// ── lttb_indices ───────────────────────────────────────────────────────

/// A budget at or above the run length, and every degenerate budget
/// (0/1/2), must return the identity selection — the adjudicated
/// "never erase" contract for unusable budgets.
#[test]
fn lttb_budget_boundaries_never_erase() {
    let run: Vec<(usize, f32)> = (0..8).map(|index| (index, index as f32)).collect();
    let identity: Vec<usize> = (0..8).collect();
    assert_eq!(lttb_indices(&run, 8), identity, "budget == len is identity");
    assert_eq!(
        lttb_indices(&run, 100),
        identity,
        "budget above len is identity"
    );
    for degenerate in [0, 1, 2] {
        assert_eq!(
            lttb_indices(&run, degenerate),
            identity,
            "budget {degenerate} falls back to identity instead of erasing"
        );
    }
    // The empty run has no positions to select under any budget.
    for budget in [0, 1, 5] {
        assert!(lttb_indices(&[], budget).is_empty());
    }
}

/// The load-bearing shape contract: exact budget, both endpoints kept,
/// strictly increasing positions AND original indices.
#[test]
fn lttb_respects_budget_keeps_endpoints_and_stays_monotonic() {
    let run: Vec<(usize, f32)> = (0..100)
        .map(|index| (index, if index == 37 { 99.0 } else { 1.0 }))
        .collect();
    let selected = lttb_indices(&run, 10);
    assert_eq!(selected.len(), 10, "the budget is filled exactly");
    assert_eq!(selected.first(), Some(&0), "the run's first sample is kept");
    assert_eq!(selected.last(), Some(&99), "the run's last sample is kept");
    for pair in selected.windows(2) {
        assert!(pair[0] < pair[1], "positions are strictly increasing");
    }
    let indices: Vec<usize> = selected.iter().map(|&position| run[position].0).collect();
    let mut sorted = indices.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(indices, sorted, "original indices stay strictly increasing");
}

/// A spike inside the run must survive decimation — straight
/// bucket-averaging would erase it.
#[test]
fn lttb_retains_an_interior_spike() {
    let run: Vec<(usize, f32)> = (0..100)
        .map(|index| (index, if index == 37 { 99.0 } else { 1.0 }))
        .collect();
    let selected = lttb_indices(&run, 10);
    assert!(
        selected.iter().any(|&position| run[position] == (37, 99.0)),
        "the spike survives decimation"
    );
}

/// Decimation must never invert a monotonic trend: the visual direction
/// of the selected values stays honest.
#[test]
fn lttb_never_inverts_a_monotonic_trend() {
    let run: Vec<(usize, f32)> = (0..60).map(|index| (index, index as f32)).collect();
    let selected = lttb_indices(&run, 12);
    for pair in selected.windows(2) {
        assert!(
            run[pair[0]].1 < run[pair[1]].1,
            "an increasing trend stays increasing"
        );
    }
}

/// Equal values everywhere: every triangle area is zero, so the strict
/// `>` comparison keeps each bucket's FIRST candidate — a deterministic,
/// pinned selection (the tie-break contract).
#[test]
fn lttb_flat_run_ties_keep_the_first_candidate_deterministically() {
    let run: Vec<(usize, f32)> = (0..10).map(|index| (index, 7.5)).collect();
    assert_eq!(lttb_indices(&run, 5), vec![0, 1, 3, 6, 9]);
}

/// The x-coordinate is the ORIGINAL sample index, so a run whose indices
/// are sparse (the gap-adjacent spacing after run splitting) still
/// decimates under honest time-axis geometry and reports those original
/// indices back.
#[test]
fn lttb_uses_original_indices_as_x_for_sparse_runs() {
    let run = vec![
        (0usize, 1.0f32),
        (1, 2.0),
        (2, 1.0),
        (50, 9.0),
        (51, 3.0),
        (52, 1.0),
    ];
    let selected = lttb_indices(&run, 4);
    assert_eq!(selected.first(), Some(&0));
    assert_eq!(selected.last(), Some(&(run.len() - 1)));
    for &position in &selected {
        assert!(position < run.len());
    }
    let indices: Vec<usize> = selected.iter().map(|&position| run[position].0).collect();
    for pair in indices.windows(2) {
        assert!(pair[0] < pair[1], "original sparse indices stay increasing");
    }
}

// ── stride_envelope ────────────────────────────────────────────────────

/// Bucket boundaries and the max rule, pinned on the canonical
/// 10-samples/target-4 shape.
#[test]
fn stride_bucket_boundaries_take_the_maximum() {
    let samples: Vec<f32> = (0..10).map(|index| index as f32).collect();
    assert_eq!(stride_envelope(&samples, 4), vec![2.0, 5.0, 8.0, 9.0]);
    // A spike at the bucket's START stays visible (newest-of-bucket hid it).
    assert_eq!(stride_envelope(&[100.0, 1.0, 1.0], 1), vec![100.0]);
}

/// Degenerate targets and short inputs never erase: identity copy.
#[test]
fn stride_target_zero_or_short_input_is_identity() {
    let samples = vec![1.0, f32::NAN, 3.0];
    let copied = stride_envelope(&samples, 0);
    assert_eq!(copied.len(), samples.len());
    assert_eq!(copied[0], 1.0);
    assert!(copied[1].is_nan(), "an explicit gap stays a gap");
    assert_eq!(copied[2], 3.0);
    // NaN payload bits must round-trip exactly (NaN != NaN under
    // `PartialEq`, so compare bit patterns element-wise).
    let identity = stride_envelope(&samples, 3);
    assert_eq!(
        identity.len(),
        samples.len(),
        "target == len is an identity copy"
    );
    let oversized = stride_envelope(&samples, 10);
    assert_eq!(
        oversized.len(),
        samples.len(),
        "target above len is an identity copy"
    );
    for produced in [identity, oversized] {
        for (a, b) in produced.iter().zip(&samples) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }
    assert!(stride_envelope(&[], 4).is_empty());
}

/// Gap semantics: an all-gap bucket stays an explicit NaN gap; a mixed
/// bucket keeps the maximum of its finite values (a gap beside a real
/// reading must not erase the reading, and vice versa).
#[test]
fn stride_preserves_gap_buckets_and_finite_maxima() {
    let gapped = vec![f32::NAN, f32::NAN, 2.0, 3.0];
    let down = stride_envelope(&gapped, 2);
    assert_eq!(down.len(), 2);
    assert!(down[0].is_nan(), "an all-gap bucket stays a gap");
    assert_eq!(down[1], 3.0);
    let mixed = vec![f32::NAN, 7.0, f32::NAN, 2.0];
    assert_eq!(stride_envelope(&mixed, 1), vec![7.0]);
}

#[test]
fn stride_positions_keep_timestamps_aligned_with_selected_values() {
    let samples = vec![1.0, 9.0, f32::NAN, f32::NAN, f32::NAN, 7.0];
    let positions = stride_envelope_positions(&samples, 3);
    let values = positions
        .iter()
        .map(|position| samples[*position])
        .collect::<Vec<_>>();
    assert_eq!(positions, vec![1, 3, 5]);
    assert_eq!(values[0], 9.0);
    assert!(values[1].is_nan());
    assert_eq!(values[2], 7.0);
    assert_eq!(values.len(), stride_envelope(&samples, 3).len());
}

/// The output length never exceeds the target for any (len, target)
/// combination — the budget contract.
#[test]
fn stride_output_never_exceeds_the_target() {
    for len in [1, 2, 9, 10, 11, 100, 101] {
        let samples: Vec<f32> = (0..len).map(|index| index as f32 % 13.0).collect();
        for target in [1, 2, 3, 4, 7, 10] {
            if len <= target {
                continue;
            }
            let down = stride_envelope(&samples, target);
            assert!(
                down.len() <= target,
                "len {len} target {target} produced {}",
                down.len()
            );
            assert!(!down.is_empty());
        }
    }
}

/// Both kernels are single-pass O(n) by construction (every bucket scan
/// touches disjoint slices whose total is the input length), so a 600k
/// point input — a 7d window at live cadence — completes in milliseconds.
/// No wall-clock assertion is made on purpose: a timing bound adds CI
/// flake on loaded runners without guarding anything the disjoint-slice
/// structure does not already guarantee, and a complexity regression
/// would also blow up the dedicated performance gates. This test still
/// executes the full 600k path so a hang or panic cannot hide.
#[test]
fn large_input_600k_points_completes_with_honest_output() {
    let len = 600_000;
    let samples: Vec<f32> = (0..len)
        .map(|index| {
            if index % 997 == 0 {
                99.0
            } else {
                (index % 37) as f32
            }
        })
        .collect();
    let run: Vec<(usize, f32)> = samples
        .iter()
        .enumerate()
        .map(|(index, &value)| (index, value))
        .collect();

    let selected = lttb_indices(&run, 600);
    assert_eq!(selected.len(), 600, "the LTTB budget is filled exactly");
    assert_eq!(selected.first(), Some(&0));
    assert_eq!(selected.last(), Some(&(len - 1)));

    let down = stride_envelope(&samples, 600);
    assert!(down.len() <= 600, "the stride budget is respected");
    assert!(down.iter().all(|value| value.is_finite()));
}
