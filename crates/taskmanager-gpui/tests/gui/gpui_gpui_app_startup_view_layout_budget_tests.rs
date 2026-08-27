use super::{StartupPageBudget, TimelinePresentation};
use crate::gpui_app::list_view::SourceNoticePresentation;
use crate::gpui_app::root::responsive::{
    LayoutProfile, NavigationPresentation, PageLayoutBudget, VerticalSpace,
};
use gpui::{px, size};

#[test]
fn startup_allocation_keeps_width_and_height_as_independent_facts() {
    let constrained_wide = StartupPageBudget::from_page_layout(PageLayoutBudget::for_viewport(
        size(px(1920.0), px(540.0)),
    ));
    assert_eq!(constrained_wide.timeline, TimelinePresentation::Collapsed);
    assert_eq!(
        constrained_wide.source_notice,
        SourceNoticePresentation::Compact
    );

    let standard = StartupPageBudget::from_page_layout(PageLayoutBudget::for_viewport(size(
        px(1180.0),
        px(780.0),
    )));
    assert_eq!(
        standard.timeline,
        TimelinePresentation::Expanded { row_limit: 4 }
    );
    assert_eq!(standard.source_notice, SourceNoticePresentation::Standard);

    let wide = StartupPageBudget::from_page_layout(PageLayoutBudget::for_viewport(size(
        px(1920.0),
        px(1080.0),
    )));
    assert_eq!(
        wide.timeline,
        TimelinePresentation::SidePanel { row_limit: 16 }
    );
}

#[test]
fn every_startup_profile_state_has_one_bounded_timeline_allocation() {
    let profiles = [
        LayoutProfile::UltraCompact,
        LayoutProfile::Compact,
        LayoutProfile::Standard,
        LayoutProfile::Wide,
    ];
    let cases = [
        (
            VerticalSpace::Constrained,
            [
                TimelinePresentation::Collapsed,
                TimelinePresentation::Collapsed,
                TimelinePresentation::Collapsed,
                TimelinePresentation::Collapsed,
            ],
        ),
        (
            VerticalSpace::Standard,
            [
                TimelinePresentation::Expanded { row_limit: 2 },
                TimelinePresentation::Expanded { row_limit: 3 },
                TimelinePresentation::Expanded { row_limit: 4 },
                TimelinePresentation::SidePanel { row_limit: 10 },
            ],
        ),
        (
            VerticalSpace::Generous,
            [
                TimelinePresentation::Expanded { row_limit: 4 },
                TimelinePresentation::Expanded { row_limit: 6 },
                TimelinePresentation::Expanded { row_limit: 8 },
                TimelinePresentation::SidePanel { row_limit: 16 },
            ],
        ),
    ];
    for (vertical_space, expected) in cases {
        for (profile, expected_timeline) in profiles.into_iter().zip(expected) {
            let layout = PageLayoutBudget {
                profile,
                vertical_space,
                page_padding: 16.0,
                navigation: NavigationPresentation::Labeled,
            };
            let presentation = StartupPageBudget::from_page_layout(layout);
            assert_eq!(presentation.timeline, expected_timeline);
            assert_eq!(
                presentation.source_notice,
                match vertical_space {
                    VerticalSpace::Constrained => SourceNoticePresentation::Compact,
                    VerticalSpace::Standard | VerticalSpace::Generous => {
                        SourceNoticePresentation::Standard
                    }
                }
            );
            assert!(presentation.table_min_height >= 168.0);
        }
    }
}
