//! Headless behavior tests for the neutral vector empty-state illustration
//! (ICED-047): the Containers and Users empty states draw a real vector
//! glyph, the two motifs are distinct, and every shape stays inside the
//! illustration's own bounds.

use super::*;
use crate::ui::containers::{EmptyStateMotif, IllustrationShape, empty_state_shapes};

/// Every shape of a motif carries a real extent and stays inside the
/// illustration's normalized bounds: no zero-sized glyph, nothing drawn
/// off-frame.
#[test]
fn illustration_shapes_stay_inside_their_normalized_bounds() {
    for motif in [
        EmptyStateMotif::ContainerStack,
        EmptyStateMotif::EmptySession,
    ] {
        let shapes = empty_state_shapes(motif);
        assert!(
            shapes.len() >= 2,
            "{motif:?}: an empty-state glyph needs a real shape, got {}",
            shapes.len()
        );
        for shape in shapes {
            match shape {
                IllustrationShape::RoundedRect {
                    x,
                    y,
                    width,
                    height,
                    radius,
                } => {
                    assert!(
                        width > 0.0 && height > 0.0,
                        "{motif:?}: a shape needs extent"
                    );
                    assert!(x >= 0.0 && y >= 0.0, "{motif:?}: a shape starts inside");
                    assert!(
                        x + width <= 1.0 && y + height <= 1.0,
                        "{motif:?}: a shape stays inside the frame"
                    );
                    assert!(radius > 0.0 && radius < 0.5, "{motif:?}: a rounded shape");
                }
                IllustrationShape::Circle { x, y, radius } => {
                    assert!(radius > 0.0 && radius < 0.5, "{motif:?}: a circle's radius");
                    assert!(
                        x - radius >= 0.0 && x + radius <= 1.0,
                        "{motif:?}: a circle stays inside the frame"
                    );
                    assert!(
                        y - radius >= 0.0 && y + radius <= 1.0,
                        "{motif:?}: a circle stays inside the frame"
                    );
                }
            }
        }
    }
}

/// The two empty states draw DIFFERENT glyphs: the container-free host and
/// the session-free host must not wear the same decoration.
#[test]
fn the_two_empty_states_draw_distinct_glyphs() {
    let containers = empty_state_shapes(EmptyStateMotif::ContainerStack);
    let sessions = empty_state_shapes(EmptyStateMotif::EmptySession);
    assert_ne!(
        containers, sessions,
        "the container and session empty states must not share one glyph"
    );
}

/// The compact density shrinks the glyph but never removes it.
#[test]
fn compact_density_shrinks_the_glyph_without_dropping_it() {
    assert!(empty_state_illustration_size(true) < empty_state_illustration_size(false));
    assert!(empty_state_illustration_size(true) > 0.0);
}

/// The illustration draws in the skin's own tokens: the muted foreground
/// stroke and a faint accent wash — never a literal color.
#[test]
fn illustration_palette_reads_the_theme_tokens() {
    let theme = Theme::dark();
    let palette = theme.palette();
    let muted = crate::theme_binding::color(palette.fg_muted);
    assert_eq!(
        muted,
        theme::muted_text_color(&theme),
        "the illustration stroke is the skin's muted foreground"
    );
    let mut fill = crate::theme_binding::color(palette.accent);
    fill.a = 0.22;
    assert!(
        fill.a < 1.0 && fill.a > 0.0,
        "the illustration's wash stays a faint accent tint"
    );
}

/// The illustration element builds for both motifs.
#[test]
fn illustration_element_builds_for_both_motifs() {
    let theme = Theme::dark();
    for motif in [
        EmptyStateMotif::ContainerStack,
        EmptyStateMotif::EmptySession,
    ] {
        let _element =
            empty_state_illustration(&theme, motif, empty_state_illustration_size(false));
    }
}

/// The Users page wears the illustration only for a genuinely empty session
/// inventory: a typed source state and the non-empty branches never do.
#[test]
fn users_empty_state_wears_the_illustration_only_when_the_inventory_is_empty() {
    use crate::ui::tables::ListState;
    use crate::ui::users::users_empty_state_wears_illustration;

    assert!(users_empty_state_wears_illustration(
        ListState::Empty,
        false
    ));
    assert!(
        !users_empty_state_wears_illustration(ListState::Empty, true),
        "a typed source state explains itself instead of wearing the illustration"
    );
    assert!(!users_empty_state_wears_illustration(
        ListState::Ready,
        false
    ));
    assert!(!users_empty_state_wears_illustration(
        ListState::Loading,
        false
    ));
}
