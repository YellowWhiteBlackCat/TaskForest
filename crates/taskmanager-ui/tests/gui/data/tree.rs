//! Tree component unit tests (line split).

mod tests_inner {
    use crate::data::tree::{MoveDirection, Tree, TreeItem, TreePath, TreeState};
    use gpui::{
        AppContext, Context, IntoElement, ParentElement, Render, Styled, TestAppContext, Window,
        div,
    };
    use taskmanager_theme::Theme;

    fn sample_roots() -> Vec<TreeItem> {
        vec![
            TreeItem::new("src", "src").expanded(true).child(
                TreeItem::new("src/ui", "ui")
                    .expanded(true)
                    .child(TreeItem::new("src/ui/button.rs", "button.rs"))
                    .child(TreeItem::new("src/ui/icon.rs", "icon.rs")),
            ),
            TreeItem::new("src/lib.rs", "lib.rs"),
            TreeItem::new("Cargo.toml", "Cargo.toml").disabled(true),
            TreeItem::new("README.md", "README.md"),
        ]
    }

    fn labels(state: &TreeState) -> Vec<String> {
        state
            .entries()
            .iter()
            .filter_map(|e| state.item_for_path(&e.path))
            .map(|i| i.label.to_string())
            .collect()
    }

    #[gpui::test]
    async fn flatten_respects_initial_expansion(cx: &mut TestAppContext) {
        let state = cx.new(|cx| {
            let mut s = TreeState::new(cx);
            s.set_items(sample_roots(), cx);
            s
        });
        state.read_with(cx, |s, _| {
            assert_eq!(
                labels(s),
                vec![
                    "src",
                    "ui",
                    "button.rs",
                    "icon.rs",
                    "lib.rs",
                    "Cargo.toml",
                    "README.md"
                ]
            );
            assert_eq!(s.entries()[1].depth, 1);
            assert_eq!(s.entries()[1].path.segments(), &[0, 0]);
        });
    }

    #[gpui::test]
    async fn collapse_hides_descendants_and_reparents_selection(cx: &mut TestAppContext) {
        let state = cx.new(|cx| {
            let mut s = TreeState::new(cx);
            s.set_items(sample_roots(), cx);
            s
        });
        state.update(cx, |s, cx| {
            // Select a deep descendant of src/ui (button.rs).
            s.set_selected_index(Some(2), cx);
            assert_eq!(s.selected_path().unwrap().segments(), &[0, 0, 0]);
            // Collapse "src" (path [0]): descendants vanish and the
            // selection re-parents onto "src" itself.
            assert!(s.collapse(&TreePath::from_segments(&[0]), cx));
            assert_eq!(s.selected_index(), Some(0));
            assert_eq!(s.selected_path().unwrap().segments(), &[0]);
            assert_eq!(labels(s), vec!["src", "lib.rs", "Cargo.toml", "README.md"]);
        });
    }

    #[gpui::test]
    async fn expand_leaf_is_noop_and_false(cx: &mut TestAppContext) {
        let state = cx.new(|cx| {
            let mut s = TreeState::new(cx);
            s.set_items(sample_roots(), cx);
            s
        });
        state.update(cx, |s, cx| {
            assert!(!s.expand(&TreePath::from_segments(&[1]), cx), "leaf");
            assert!(!s.expand(&TreePath::from_segments(&[9]), cx), "missing");
            // Collapsing a collapsed folder is also a no-op.
            assert!(!s.collapse(&TreePath::from_segments(&[2]), cx));
        });
    }

    #[gpui::test]
    async fn keyboard_left_right_and_confirm(cx: &mut TestAppContext) {
        let state = cx.new(|cx| {
            let mut s = TreeState::new(cx);
            s.set_items(sample_roots(), cx);
            s
        });
        state.update(cx, |s, cx| {
            s.set_selected_index(Some(0), cx);
            // Enter on the expanded folder collapses it.
            s.confirm(cx);
            assert!(!s.is_expanded(&TreePath::from_segments(&[0])));
            // Right expands it again.
            s.select_right(cx);
            assert!(s.is_expanded(&TreePath::from_segments(&[0])));
            // Left on the expanded folder collapses it.
            s.select_left(cx);
            assert!(!s.is_expanded(&TreePath::from_segments(&[0])));
            // Left on a leaf selects its parent (deviation, tested). The
            // tree is collapsed from the step above, so expand it again,
            // then move onto the leaf "src/ui/icon.rs" (index 3).
            s.select_right(cx);
            assert!(s.is_expanded(&TreePath::from_segments(&[0])));
            s.set_selected_index(Some(3), cx);
            s.select_left(cx);
            assert_eq!(s.selected_index(), Some(1));
            // Right on an expanded folder selects its first child.
            s.select_right(cx);
            assert_eq!(s.selected_index(), Some(2));
        });
    }

    #[gpui::test]
    async fn move_selection_skips_disabled_and_wraps(cx: &mut TestAppContext) {
        let state = cx.new(|cx| {
            let mut s = TreeState::new(cx);
            s.set_items(sample_roots(), cx);
            s
        });
        state.update(cx, |s, cx| {
            // Flat ix 5 is the disabled Cargo.toml: Down from 4 must skip
            // it and land on 6.
            s.set_selected_index(Some(4), cx);
            s.move_selection(MoveDirection::Down, cx);
            assert_eq!(s.selected_index(), Some(6));
            // Wrap: Down from the last lands on 0.
            s.move_selection(MoveDirection::Down, cx);
            assert_eq!(s.selected_index(), Some(0));
            // Up wraps to the last non-disabled entry.
            s.move_selection(MoveDirection::Up, cx);
            assert_eq!(s.selected_index(), Some(6));
        });
    }

    #[gpui::test]
    async fn set_selected_index_clamps_to_visible(cx: &mut TestAppContext) {
        let state = cx.new(|cx| {
            let mut s = TreeState::new(cx);
            s.set_items(sample_roots(), cx);
            s
        });
        state.update(cx, |s, cx| {
            s.set_selected_index(Some(999), cx);
            assert_eq!(s.selected_index(), Some(6));
            s.set_selected_index(None, cx);
            assert_eq!(s.selected_index(), None);
        });
    }

    #[gpui::test]
    async fn tree_renders_with_default_row(cx: &mut TestAppContext) {
        struct Harness {
            state: gpui::Entity<TreeState>,
        }
        impl Render for Harness {
            fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
                div()
                    .size_full()
                    .child(Tree::new(&self.state, Theme::dark().palette()))
            }
        }
        let state = cx.new(|cx| {
            let mut s = TreeState::new(cx);
            s.set_items(sample_roots(), cx);
            s
        });
        let window = cx.add_window(|_window, _cx| Harness {
            state: state.clone(),
        });
        let _ = cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear());
    }

    #[test]
    fn tree_path_parent_and_child() {
        let root = TreePath::new();
        let a = root.child(2);
        assert_eq!(a.segments(), &[2]);
        assert_eq!(a.depth(), 1);
        let b = a.child(0);
        assert_eq!(b.segments(), &[2, 0]);
        assert_eq!(b.parent(), Some(a.clone()));
        assert_eq!(a.parent(), Some(TreePath::new()));
        assert_eq!(root.parent(), None);
        assert_eq!(TreePath::from_segments(&[1, 2]).segments(), &[1, 2]);
    }
}
