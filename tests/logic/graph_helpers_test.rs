//! Pure-logic tests for headless-math helpers in the gpui layer that are cheap to
//! assert without a window:
//! - `graph::compute_column_count` — the Mission Center core-grid column chooser.
//! - `cpu_view::format_uptime` — the system-uptime `D:HH:MM:SS` / `HH:MM:SS` formatter.
//!
//! Private helpers that WOULD belong here but cannot be reached from `tests/`
//! without editing `src/` (out of this agent's file ownership) — noted as gaps:
//! - `graph::sample_at_cursor_x` (cursor x → nearest sample index; private, but
//!   already unit-tested inside `graph.rs`'s own `#[cfg(test)] mod`).
//! - `perf_views::memory_composition_segments` / `stacked_bar` share math (private).
//! - `perf_views::rate_str` / `sidebar::rate_str` bytes→rate formatters (private).

use taskmanager_gpui::gpui_app::cpu_view::format_uptime;
use taskmanager_gpui::gpui_app::graph::compute_column_count;

// ── compute_column_count ─────────────────────────────────────────────────────
//
// Mission Center recipe: choose grid columns for N logical processors.
//   N <= 3                -> N (clamped to >= 1)
//   else                  -> first divisor of N inside [round(sqrt(N)), min(N, 2*round(sqrt(N)))]
//                            falling back to round(sqrt(N)) when none divides.

/// Small N (<=3) returns N directly; the floor `n.max(1)` means 0 cores still
/// yields 1 column (no empty / zero-column grid).
#[test]
fn compute_column_count_small_n_returns_n_clamped_to_one() {
    assert_eq!(compute_column_count(0), 1);
    assert_eq!(compute_column_count(1), 1);
    assert_eq!(compute_column_count(2), 2);
    assert_eq!(compute_column_count(3), 3);
}

/// The canonical MC examples from the doc comment: 20 cores → 4 cols (4×5),
/// 24 cores → 6 cols (6×4). Pins the recipe's headline values.
#[test]
fn compute_column_count_documented_examples() {
    assert_eq!(compute_column_count(20), 4); // 4×5
    assert_eq!(compute_column_count(24), 6); // 6×4
}

/// Square counts pick the integer root (a perfect-square core count tiles into a
/// square grid): 4→2, 9→3, 16→4, 36→6, 64→8.
#[test]
fn compute_column_count_perfect_squares_pick_root() {
    assert_eq!(compute_column_count(4), 2);
    assert_eq!(compute_column_count(9), 3);
    assert_eq!(compute_column_count(16), 4);
    assert_eq!(compute_column_count(36), 6);
    assert_eq!(compute_column_count(64), 8);
}

/// Composite counts whose sqrt-neighborhood holds a divisor: 6→2, 8→4, 12→3,
/// 28→7, 32→8, 48→8. Each is the first divisor in [round(sqrt(N)), 2·round(sqrt(N))].
#[test]
fn compute_column_count_composite_counts() {
    assert_eq!(compute_column_count(6), 2);
    assert_eq!(compute_column_count(8), 4);
    assert_eq!(compute_column_count(12), 3);
    assert_eq!(compute_column_count(28), 7); // 7×4
    assert_eq!(compute_column_count(32), 8); // 8×4
    assert_eq!(compute_column_count(48), 8); // 8×6
}

/// A prime core count has no divisor in range, so the function falls back to
/// `round(sqrt(N))`. 7 → round(2.65) = 3 (3×… with a remainder, but the recipe
/// returns the base rather than failing). 11 → round(3.32) = 3. 13 → round(3.61) = 4.
#[test]
fn compute_column_count_prime_falls_back_to_sqrt_round() {
    assert_eq!(compute_column_count(7), 3);
    assert_eq!(compute_column_count(11), 3);
    assert_eq!(compute_column_count(13), 4);
}

/// The result is always a positive integer in a sane range (>=1, <= N) for a
/// broad sweep — guards against any future regression returning 0 or a column
/// count larger than the core count.
#[test]
fn compute_column_count_always_positive_and_le_n() {
    for n in [
        1usize, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 16, 20, 24, 32, 48, 64, 96, 128,
    ] {
        let c = compute_column_count(n);
        assert!(c >= 1, "n={n}: column count must be >= 1, got {c}");
        assert!(c <= n, "n={n}: column count must be <= n, got {c}");
    }
}

// ── format_uptime ────────────────────────────────────────────────────────────
//
// `format_uptime(secs)`:
//   secs < 1 day  -> "HH:MM:SS"
//   secs >= 1 day -> "D:HH:MM:SS"  (day count unpadded, the rest zero-padded)

#[test]
fn format_uptime_zero() {
    assert_eq!(format_uptime(0), "00:00:00");
}

#[test]
fn format_uptime_seconds_only() {
    assert_eq!(format_uptime(59), "00:00:59");
}

#[test]
fn format_uptime_minutes_and_seconds() {
    assert_eq!(format_uptime(125), "00:02:05"); // 2m 5s
}

#[test]
fn format_uptime_exactly_one_hour() {
    assert_eq!(format_uptime(3_600), "01:00:00");
}

#[test]
fn format_uptime_last_second_of_a_day() {
    assert_eq!(format_uptime(86_399), "23:59:59");
}

#[test]
fn format_uptime_first_second_of_day_two_switches_to_day_format() {
    // 86_400s = exactly 1 day → the `d > 0` branch engages, day count unpadded.
    assert_eq!(format_uptime(86_400), "1:00:00:00");
}

#[test]
fn format_uptime_day_plus_hours_minutes_seconds() {
    // 1 day + 1h + 1m + 1s
    assert_eq!(format_uptime(86_400 + 3_661), "1:01:01:01");
}

#[test]
fn format_uptime_multi_day_day_field_unpadded() {
    // 3 days + 2h + 30m + 5s — day field has no leading zero.
    assert_eq!(
        format_uptime(3 * 86_400 + 2 * 3_600 + 30 * 60 + 5),
        "3:02:30:05"
    );
}
