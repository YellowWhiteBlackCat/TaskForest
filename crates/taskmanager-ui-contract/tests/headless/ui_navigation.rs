use super::*;
use taskmanager_application::AppAction;

#[test]
fn descriptors_follow_the_application_page_order_without_a_second_page_list() {
    let descriptors = page_descriptors();
    let pages = descriptors.map(|item| item.page);

    assert_eq!(pages, AppPage::ALL);
    for item in descriptors {
        assert_eq!(item.command.action(), select_page_action(item.page));
        assert!(matches!(item.label, MessageKey::CommandLabel(_)));
        assert!(matches!(
            item.description,
            MessageKey::CommandDescription(_)
        ));
        assert!(page_key_chord(item.page).is_some());
    }
}

#[test]
fn page_commands_have_distinct_icons_and_default_shortcuts() {
    let descriptors = page_descriptors();

    for (index, item) in descriptors.iter().enumerate() {
        assert!(
            descriptors[index + 1..]
                .iter()
                .all(|other| other.command != item.command)
        );
        assert!(
            descriptors[index + 1..]
                .iter()
                .all(|other| other.icon != item.icon)
        );
        assert!(page_shortcut(item.page).is_some());
    }
}

const fn select_page_action(page: AppPage) -> AppAction {
    AppAction::SelectPage(page)
}
