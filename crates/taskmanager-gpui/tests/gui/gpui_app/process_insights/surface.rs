use super::card;
use gpui::Styled;
use taskmanager_theme::{GlassSurfaceRole, Theme};
use taskmanager_ui::theme_binding::glass_fill;

#[test]
fn insight_cards_use_the_dialog_nested_surface_without_a_second_shadow() {
    let theme = Theme::dark();
    let mut panel = card(&theme, "Network throughput", 480.0);
    let tier = GlassSurfaceRole::InlinePlot.tier();

    assert_eq!(
        panel.style().background.as_ref(),
        Some(&glass_fill(&theme, tier)),
        "insight panels must spend the shared inset glass material"
    );
    assert!(
        panel
            .style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty()),
        "a panel inside the properties dialog must not stack a second shadow"
    );
}
