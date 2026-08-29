use super::*;
use taskmanager_core::core::process::PriorityTier;

#[test]
fn priority_choice_maps_to_typed_priority_tiers_and_localizes() {
    use taskmanager_core::core::process::ProcessBatchAction;
    // The typed tier carries the semantics (the adapter owns the
    // native-primitive mapping); the presets must match GPUI's action bar
    // so identically-labeled buttons never diverge.
    let expected = [
        (PriorityChoice::High, PriorityTier::High),
        (PriorityChoice::Normal, PriorityTier::Normal),
        (PriorityChoice::Low, PriorityTier::Low),
    ];
    for (choice, tier) in expected {
        assert_eq!(choice.action(), ProcessBatchAction::SetPriority(tier));
    }
    assert_eq!(PriorityChoice::ALL.len(), expected.len());
    // The pick_list labels resolve through the shared catalog.
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    assert_eq!(PriorityChoice::High.to_string(), "High");
    assert_eq!(PriorityChoice::Normal.to_string(), "Normal");
    assert_eq!(PriorityChoice::Low.to_string(), "Low");
}

#[test]
fn selection_hint_surfaces_the_multi_select_count_only_past_one() {
    // Localized copy: pin English so the assertion is identical on every
    // runner regardless of the host locale (the shared t() auto-seeds
    // from the host language otherwise).
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    // Zero / one selection keeps the legacy single-row note; a batch of two
    // or more surfaces the count so the user knows a verb reaches N rows.
    assert_eq!(selection_hint(0), "Delete confirms");
    assert_eq!(selection_hint(1), "Delete confirms");
    assert_eq!(selection_hint(3), "3 selected · Delete confirms");
    assert_eq!(selection_hint(12), "12 selected · Delete confirms");
}
