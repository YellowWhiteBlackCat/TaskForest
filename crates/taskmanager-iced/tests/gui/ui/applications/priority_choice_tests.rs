use super::*;
use taskforest_core::core::process::PriorityTier;
use taskmanager_application::i18n::Language;
use taskmanager_application::i18n::set_language;

#[test]
fn priority_choice_maps_to_typed_priority_tiers_and_localizes() {
    use taskforest_core::core::process::ProcessBatchAction;
    // The core owns the ORDER and the batch action; the shell owns the
    // wording fold. The local option is only the toolkit adapter that
    // carries the core tier, so no frontend spells a tier set of its own.
    let options = PriorityOption::all();
    assert_eq!(options.len(), PriorityTier::ALL.len());
    for (option, tier) in options.iter().zip(PriorityTier::ALL) {
        assert_eq!(option.tier, tier);
        assert_eq!(option.action(), ProcessBatchAction::SetPriority(tier));
    }
    // The pick_list labels resolve through the shell's one wording fold.
    set_language(Language::En);
    for (option, tier) in options.iter().zip(PriorityTier::ALL) {
        assert_eq!(option.to_string(), priority_tier_label(tier));
    }
    assert_eq!(
        options[0].to_string(),
        priority_tier_label(PriorityTier::High)
    );
}

#[test]
fn selection_hint_surfaces_the_multi_select_count_only_past_one() {
    // Localized copy: pin English so the assertion is identical on every
    // runner regardless of the host locale (the shared t() auto-seeds
    // from the host language otherwise).
    set_language(Language::En);
    // Zero / one selection keeps the legacy single-row note; a batch of two
    // or more surfaces the count so the user knows a verb reaches N rows.
    assert_eq!(selection_hint(0), "Delete confirms");
    assert_eq!(selection_hint(1), "Delete confirms");
    assert_eq!(selection_hint(3), "3 selected · Delete confirms");
    assert_eq!(selection_hint(12), "12 selected · Delete confirms");
}
