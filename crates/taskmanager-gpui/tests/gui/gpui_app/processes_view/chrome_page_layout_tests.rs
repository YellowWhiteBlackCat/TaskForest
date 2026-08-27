use super::page_layout::{
    ProcessActionPresentation, ProcessActionSurface, ProcessChromePresentation,
    ProcessControlPresentation, ProcessOverviewPresentation,
};
use crate::gpui_app::root::{
    LayoutProfile, NavigationPresentation, PageLayoutBudget, VerticalSpace,
};

fn page_layout(profile: LayoutProfile, vertical_space: VerticalSpace) -> PageLayoutBudget {
    PageLayoutBudget {
        profile,
        vertical_space,
        page_padding: 16.0,
        navigation: NavigationPresentation::Labeled,
    }
}

#[test]
fn every_apps_profile_state_has_one_explicit_chrome_allocation() {
    for profile in [
        LayoutProfile::UltraCompact,
        LayoutProfile::Compact,
        LayoutProfile::Standard,
        LayoutProfile::Wide,
    ] {
        for vertical_space in [
            VerticalSpace::Constrained,
            VerticalSpace::Standard,
            VerticalSpace::Generous,
        ] {
            let presentation =
                ProcessChromePresentation::from_page_layout(page_layout(profile, vertical_space));
            let expected_overview = match vertical_space {
                VerticalSpace::Constrained => ProcessOverviewPresentation::TitleAndSearch,
                VerticalSpace::Standard | VerticalSpace::Generous => {
                    ProcessOverviewPresentation::SummaryAndSearch
                }
            };
            let expected_controls = match profile {
                LayoutProfile::Wide => ProcessControlPresentation::Unified,
                LayoutProfile::UltraCompact | LayoutProfile::Compact | LayoutProfile::Standard => {
                    ProcessControlPresentation::Stacked
                }
            };
            let expected_actions = match (profile, vertical_space) {
                (
                    LayoutProfile::Standard | LayoutProfile::Wide,
                    VerticalSpace::Standard | VerticalSpace::Generous,
                ) => ProcessActionPresentation::Primary,
                _ => ProcessActionPresentation::Essential,
            };
            let expected_surface = match expected_controls {
                ProcessControlPresentation::Unified => ProcessActionSurface::Embedded,
                ProcessControlPresentation::Stacked => ProcessActionSurface::Standalone,
            };
            let expected_search_width = match profile {
                LayoutProfile::UltraCompact | LayoutProfile::Compact => 260.0,
                LayoutProfile::Standard | LayoutProfile::Wide => 320.0,
            };
            assert_eq!(presentation.overview(), expected_overview);
            assert_eq!(presentation.controls(), expected_controls);
            assert_eq!(presentation.actions(), expected_actions);
            assert_eq!(presentation.action_surface(), expected_surface);
            assert_eq!(presentation.search_width(), expected_search_width);
        }
    }
}
