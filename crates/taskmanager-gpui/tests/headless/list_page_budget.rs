//! Pure slot-budget proof for the inventory pages' header degradation.

use super::{ListPageBudget, ListPageRhythm};
use taskmanager_theme::tokens;
use taskmanager_ui_contract::presentation::budget::PageLayoutBudget;
use taskmanager_ui_contract::presentation::vocabulary::{
    BOTTOM_SAFETY_BAND, LayoutProfile, NavigationPresentation, VerticalSpace,
};

fn layout(content_height: f32, vertical_space: VerticalSpace) -> PageLayoutBudget {
    PageLayoutBudget {
        profile: LayoutProfile::UltraCompact,
        vertical_space,
        page_padding: 16.0,
        navigation: NavigationPresentation::IconOnly,
        content_width: 688.0,
        content_height,
        chrome: None,
    }
}

#[test]
fn constrained_frames_admit_only_the_mandatory_header_group() {
    let budget = ListPageBudget::from_page_layout(
        layout(120.0, VerticalSpace::Constrained),
        tokens::UiSize::Standard,
        tokens::RowDensity::default(),
    );
    assert_eq!(budget.rows, 2);
    assert!(!budget.admit_optional_group);
    assert_eq!(budget.rhythm, ListPageRhythm::shared());
    assert!(budget.table_height < 120.0);
    assert!(budget.table_height < BOTTOM_SAFETY_BAND + tokens::SPACE_8.0);
}

#[test]
fn a_generous_frame_admits_the_optional_group_as_a_whole() {
    let budget = ListPageBudget::from_page_layout(
        layout(600.0, VerticalSpace::Generous),
        tokens::UiSize::Standard,
        tokens::RowDensity::default(),
    );
    assert_eq!(budget.rows, 3);
    assert!(budget.admit_optional_group);
    assert_eq!(budget.rhythm, ListPageRhythm::shared());
    let mandatory = ListPageBudget::from_page_layout(
        PageLayoutBudget {
            vertical_space: VerticalSpace::Constrained,
            ..layout(600.0, VerticalSpace::Generous)
        },
        tokens::UiSize::Standard,
        tokens::RowDensity::default(),
    );
    assert!(budget.table_height < mandatory.table_height);
    assert_eq!(
        mandatory.table_height - budget.table_height,
        tokens::UiSize::Standard.control_height().0 + budget.rhythm.gap
    );
}

#[test]
fn budget_is_a_pure_fold_of_the_shared_page_slot() {
    let slot = layout(540.0, VerticalSpace::Standard);
    let budget =
        ListPageBudget::from_page_layout(slot, tokens::UiSize::Large, tokens::RowDensity::Compact);
    assert_eq!(
        budget,
        ListPageBudget::from_page_layout(slot, tokens::UiSize::Large, tokens::RowDensity::Compact)
    );
}

#[test]
fn the_mandatory_rung_spends_the_one_shared_rhythm() {
    let budget = ListPageBudget::mandatory_only();
    assert_eq!(budget.rows, 2);
    assert!(!budget.admit_optional_group);
    assert_eq!(budget.rhythm.gap, tokens::SPACE_8.0);
    assert_eq!(budget.rhythm.inset, tokens::SPACE_6.0);
    let generous = ListPageBudget::from_page_layout(
        layout(600.0, VerticalSpace::Generous),
        tokens::UiSize::Standard,
        tokens::RowDensity::default(),
    );
    assert_eq!(
        generous.without_optional_group().table_height - generous.table_height,
        tokens::UiSize::Standard.control_height().0 + budget.rhythm.gap
    );
}
