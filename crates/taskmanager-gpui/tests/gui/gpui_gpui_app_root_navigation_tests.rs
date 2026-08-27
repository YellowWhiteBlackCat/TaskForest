use super::TopPage;
use taskmanager_ui_contract::page_descriptors;

#[test]
fn every_shared_page_round_trips_through_the_gpui_adapter() {
    for descriptor in page_descriptors() {
        let top_page = TopPage::from_app_page(descriptor.page);
        assert_eq!(top_page.app_page(), Some(descriptor.page));
    }
}

#[test]
fn containers_stays_outside_the_shared_page_contract() {
    assert_eq!(TopPage::Containers.app_page(), None);
}
