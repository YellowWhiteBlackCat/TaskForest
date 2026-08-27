use super::*;

#[test]
fn canonical_category_label_localizes_through_the_shared_catalog() {
    use taskmanager_application::i18n::{Language, set_language};
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    assert_eq!(t("proc.mode_category_tree"), "Categories · Tree");
    set_language(Language::Zh);
    assert_eq!(t("proc.mode_category_tree"), "分类 · 树形");
    set_language(Language::En);
}
