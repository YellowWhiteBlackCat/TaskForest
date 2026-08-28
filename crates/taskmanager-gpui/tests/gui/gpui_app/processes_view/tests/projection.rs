//! Pure projection tests for the Processes table header.
//!
//! These pin the header-navigation semantics (visible-column projection +
//! wrap-around stepping + swap-column auto-hide fallback) without touching
//! GPUI state — they exercise the pure `rows` helpers directly.

use std::collections::HashSet;

use crate::gpui_app::processes_view::rows::{
    SortCol, effective_process_hidden_cols, effective_process_sort_col, sort_col_step,
    visible_sort_cols,
};

/// The canonical header order (14 columns) with nothing hidden — the projection
/// `sort_header_row` renders by default.
fn all_visible() -> Vec<SortCol> {
    visible_sort_cols(&HashSet::new())
}

#[test]
fn visible_sort_cols_drops_hidden_columns_and_keeps_canonical_order() {
    let mut hidden = HashSet::new();
    hidden.insert(SortCol::Memory);
    hidden.insert(SortCol::DiskWrite);
    let visible = visible_sort_cols(&hidden);
    assert_eq!(
        visible,
        vec![
            SortCol::Name,
            SortCol::User,
            SortCol::Pid,
            SortCol::Threads,
            SortCol::StartTime,
            SortCol::State,
            SortCol::Cpu,
            SortCol::Swap,
            SortCol::DiskRead,
            SortCol::CpuTime,
            SortCol::Fds,
            SortCol::Nice,
        ]
    );
    // `Name` is the identity column — it survives even a fully-hidden set.
    let all_hidden: HashSet<_> = crate::gpui_app::processes_view::rows::columns()
        .iter()
        .copied()
        .collect();
    assert_eq!(visible_sort_cols(&all_hidden), vec![SortCol::Name]);
}

#[test]
fn swap_column_auto_hide_requires_confirmed_zero_not_unknown() {
    let user_visible = HashSet::new();
    let user_hidden = HashSet::from([SortCol::Memory]);

    let no_swap = effective_process_hidden_cols(&user_visible, Some(0));
    assert!(no_swap.contains(&SortCol::Swap));
    assert!(!no_swap.contains(&SortCol::Memory));

    let unknown = effective_process_hidden_cols(&user_visible, None);
    assert!(!unknown.contains(&SortCol::Swap));

    let configured = effective_process_hidden_cols(&user_hidden, Some(4096));
    assert!(!configured.contains(&SortCol::Swap));
    assert!(configured.contains(&SortCol::Memory));
}

#[test]
fn auto_hidden_active_sort_column_falls_back_to_the_first_rendered_column() {
    let hidden = HashSet::from([SortCol::Swap]);
    assert_eq!(
        effective_process_sort_col(SortCol::Swap, &hidden),
        SortCol::Name
    );
    assert_eq!(
        effective_process_sort_col(SortCol::Cpu, &hidden),
        SortCol::Cpu
    );
}

#[test]
fn sort_col_step_wraps_across_visible_columns() {
    let visible = vec![SortCol::Name, SortCol::User, SortCol::Pid];
    // Right walks toward the end and wraps to the first column.
    assert_eq!(sort_col_step(SortCol::Name, true, &visible), SortCol::User);
    assert_eq!(sort_col_step(SortCol::User, true, &visible), SortCol::Pid);
    assert_eq!(sort_col_step(SortCol::Pid, true, &visible), SortCol::Name);
    // Left walks toward the start and wraps to the last column.
    assert_eq!(sort_col_step(SortCol::Pid, false, &visible), SortCol::User);
    assert_eq!(sort_col_step(SortCol::User, false, &visible), SortCol::Name);
    assert_eq!(sort_col_step(SortCol::Name, false, &visible), SortCol::Pid);
    // A column that is NOT currently rendered (hidden via the picker) keeps
    // the current sort column — hiding the active column is a safe no-op.
    assert_eq!(
        sort_col_step(SortCol::Memory, true, &visible),
        SortCol::Memory
    );
    // Single visible column (Name after everything else is hidden): identity.
    assert_eq!(
        sort_col_step(SortCol::Name, true, &[SortCol::Name]),
        SortCol::Name
    );
    assert_eq!(
        sort_col_step(SortCol::Name, false, &[SortCol::Name]),
        SortCol::Name
    );
}

/// The default 14-column header wraps Cpu back to itself after a full cycle.
#[test]
fn sort_col_step_cycles_the_full_default_header() {
    let visible = all_visible();
    let mut col = SortCol::Cpu;
    for step in 1..=14 {
        col = sort_col_step(col, true, &visible);
        match step {
            1 => assert_eq!(col, SortCol::Memory),
            2 => assert_eq!(col, SortCol::Swap),
            3 => assert_eq!(col, SortCol::DiskRead),
            6 => assert_eq!(col, SortCol::Fds),
            7 => assert_eq!(col, SortCol::Nice),
            8 => assert_eq!(
                col,
                SortCol::Name,
                "Right past the last column wraps to Name"
            ),
            _ => {}
        }
    }
    assert_eq!(col, SortCol::Cpu, "14 rights complete a full header cycle");
    assert_eq!(
        sort_col_step(SortCol::Cpu, false, &all_visible()),
        SortCol::State
    );
}

// ── Canonical category row projection ───────────────────────────────────────

mod canonical_category {
    use std::collections::HashSet;

    use crate::core::process::{
        ProcessApplicationIdentity, ProcessCategory, ProcessItem, ProcessMetadataObservation,
    };
    use crate::gpui_app::processes_view::rows::{
        SortCol, Toggle, VisibleRowsProps, category_expansion_key, category_tree_rows, visible_rows,
    };
    use crate::i18n::{self, Language};
    use taskmanager_shell::{ProcessRowKey, ProcessStatusFilter};

    fn app_item(pid: u32, name: &str, cpu: f32, mem: u64) -> ProcessItem {
        let identity = ProcessApplicationIdentity::new("org.example.Editor", "Editor", None)
            .expect("fixture identity must be non-empty");
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .name(name.to_owned())
            .current_cpu_percentage(cpu)
            .current_memory_bytes(mem)
            .status("S".to_owned())
            .application_identity_observation(ProcessMetadataObservation::available(identity, 10))
            .build()
    }

    fn background_item(pid: u32, name: &str, cpu: f32, mem: u64) -> ProcessItem {
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .name(name.to_owned())
            .current_cpu_percentage(cpu)
            .current_memory_bytes(mem)
            .status("S".to_owned())
            .application_identity_observation(ProcessMetadataObservation::<
                ProcessApplicationIdentity,
            >::absent(10))
            .build()
    }

    fn unknown_item(pid: u32, name: &str, cpu: f32, mem: u64) -> ProcessItem {
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .name(name.to_owned())
            .current_cpu_percentage(cpu)
            .current_memory_bytes(mem)
            .status("S".to_owned())
            .build()
    }

    fn pinned_english() -> i18n::Language {
        let prior = i18n::current_language();
        i18n::set_language(Language::En);
        prior
    }

    /// One aggregate header per non-empty bucket, in the fixed order
    /// Application → Background → Uncategorized, each summing its members'
    /// CPU%/memory without fabricating a representative PID.
    #[test]
    fn renders_three_buckets_in_fixed_order_with_summed_aggregates() {
        let prior = pinned_english();
        let procs = [
            app_item(11, "editor-app", 4.0, 400),
            app_item(12, "editor-helper", 1.0, 100),
            background_item(21, "daemon", 2.5, 250),
            unknown_item(31, "mystery", 0.5, 50),
        ];
        let refs: Vec<&ProcessItem> = procs.iter().collect();
        let rows = category_tree_rows(&refs, SortCol::Cpu, false, &HashSet::new(), &HashSet::new());

        assert_eq!(rows.len(), 3, "collapsed: one header per non-empty bucket");
        assert_eq!(rows[0].name, "Applications");
        assert_eq!(rows[1].name, "Background processes");
        assert_eq!(rows[2].name, "Uncategorized");

        assert_eq!(
            rows[0].cpu,
            Some(5.0),
            "Applications CPU% is the bucket sum"
        );
        assert_eq!(
            rows[0].mem,
            Some(500),
            "Applications memory is the bucket sum"
        );
        assert_eq!(rows[0].process_pid, None);
        assert_eq!(rows[0].cell_text.pid, "");
        assert_eq!(rows[0].badge, None);
        assert_eq!(rows[1].cpu, Some(2.5));
        assert_eq!(rows[1].mem, Some(250));
        assert_eq!(rows[2].cpu, Some(0.5));
        assert_eq!(rows[2].mem, Some(50));

        // Multi-member buckets are collapsed-but-expandable headers with a
        // typed category toggle; the expansion state lives behind the row.
        assert!(rows[0].has_children);
        assert!(rows[0].collapsed);
        assert!(matches!(
            rows[0].toggle,
            Toggle::GroupCategory(ProcessCategory::Application)
        ));
        i18n::set_language(prior);
    }

    /// The honesty invariant end-to-end: an Unknown identity lands in
    /// Uncategorized — never fabricated into the confirmed-absent Background
    /// bucket (which a provider Absent observation alone proves).
    #[test]
    fn unknown_identity_lands_in_uncategorized_not_background() {
        let prior = pinned_english();
        let unknown = unknown_item(31, "mystery", 0.5, 50);
        let absent = background_item(21, "daemon", 2.0, 200);
        let rows = category_tree_rows(
            &[&unknown],
            SortCol::Name,
            true,
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Uncategorized");

        let rows = category_tree_rows(
            &[&absent],
            SortCol::Name,
            true,
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Background processes");
        i18n::set_language(prior);
    }

    /// A bucket with no processes never renders a fabricated empty header.
    #[test]
    fn empty_buckets_never_render_a_header() {
        let prior = pinned_english();
        let procs = [background_item(21, "daemon", 2.0, 200)];
        let refs: Vec<&ProcessItem> = procs.iter().collect();
        let rows = category_tree_rows(&refs, SortCol::Name, true, &HashSet::new(), &HashSet::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Background processes");

        let procs = [app_item(11, "editor", 1.0, 100)];
        let refs: Vec<&ProcessItem> = procs.iter().collect();
        let rows = category_tree_rows(&refs, SortCol::Name, true, &HashSet::new(), &HashSet::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Applications");
        i18n::set_language(prior);
    }

    /// Expansion keeps the category root, then emits one PID-less application
    /// aggregate per process-tree root, and only then emits real process rows.
    /// Untouched buckets remain collapsed.
    #[test]
    fn expansion_uses_stable_keys_and_sorts_members_by_the_active_column() {
        let prior = pinned_english();
        let procs = [
            app_item(11, "b-app", 1.0, 100),
            app_item(12, "a-app", 3.0, 300),
            background_item(21, "daemon", 2.0, 200),
        ];
        let refs: Vec<&ProcessItem> = procs.iter().collect();
        let expanded = HashSet::from([
            category_expansion_key(ProcessCategory::Application),
            "app-tree:12".to_owned(),
            "app-tree:11".to_owned(),
        ]);

        assert_eq!(
            category_expansion_key(ProcessCategory::Application),
            "category:application",
            "the expansion key is the stable category key, not the localized label"
        );

        let rows = category_tree_rows(&refs, SortCol::Cpu, false, &expanded, &HashSet::new());
        assert_eq!(
            rows.len(),
            6,
            "category header + 2 app aggregates + 2 real processes + Background header"
        );
        assert!(!rows[0].collapsed, "the expanded bucket's header flips");
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[1].name, "Editor");
        assert_eq!(rows[1].process_pid, None);
        assert_eq!(rows[1].cell_text.pid, "");
        assert_eq!(rows[2].depth, 2);
        assert_eq!(rows[2].process_pid, Some(12));
        assert_eq!(rows[3].name, "Editor");
        assert_eq!(rows[4].process_pid, Some(11));
        assert_eq!(rows[5].name, "Background processes");
        assert!(rows[5].collapsed, "untouched buckets stay collapsed");

        // The application aggregate is structural and PID-less; the process
        // row below it is the selectable real process.
        assert!(rows[1].has_children);
        assert!(matches!(rows[1].toggle, Toggle::GroupApp(_)));
        assert_eq!(rows[1].badge, None);
        assert_eq!(rows[1].cpu, Some(3.0));

        // Ascending flips the member order within the same bucket.
        let rows = category_tree_rows(&refs, SortCol::Cpu, true, &expanded, &HashSet::new());
        assert_eq!(rows[1].name, "Editor", "CPU% asc puts the 1.0% app first");
        assert_eq!(rows[2].process_pid, Some(11));
        assert_eq!(rows[3].name, "Editor");
        assert_eq!(rows[4].process_pid, Some(12));
        i18n::set_language(prior);
    }

    /// The reference hierarchy has a PID-less application total above a real
    /// process tree. This fixture deliberately gives the root and every helper
    /// the same verified launcher identity, then proves the two aggregation
    /// boundaries independently: the app row sums all members, while process
    /// parents keep their own samples and distinct PIDs.
    #[test]
    fn application_tree_uses_pidless_total_and_own_process_metrics() {
        let prior = pinned_english();
        let identity =
            ProcessApplicationIdentity::new("org.example.MissionCenter", "Mission Center", None)
                .expect("fixture identity must be non-empty");
        let item = |pid: u32, name: &str, cpu: f32, mem: u64, parent_pid: Option<u32>| {
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(pid)
                .parent_pid(parent_pid)
                .name(name.to_owned())
                .current_cpu_percentage(cpu)
                .current_memory_bytes(mem)
                .status("S".to_owned())
                .application_identity_observation(ProcessMetadataObservation::available(
                    identity.clone(),
                    10,
                ))
                .build()
        };
        let processes = [
            item(100, "missioncenter", 10.0, 1_000, None),
            item(101, "missioncenter-magpie", 2.0, 200, Some(100)),
            item(102, "bwrap", 3.0, 300, Some(100)),
            item(103, "bwrap", 4.0, 400, Some(102)),
            item(104, "glycin-svg", 5.0, 500, Some(103)),
        ];
        let refs: Vec<&ProcessItem> = processes.iter().collect();
        let expanded = HashSet::from([
            category_expansion_key(ProcessCategory::Application),
            "app-tree:100".to_owned(),
        ]);
        let rows = visible_rows(VisibleRowsProps {
            processes: &refs,
            query: "",
            sort_col: SortCol::Pid,
            sort_asc: true,
            filter: ProcessStatusFilter::All,
            collapsed: &HashSet::new(),
            expanded_apps: &expanded,
        });

        assert_eq!(rows.len(), 7, "category + app total + five process rows");
        assert_eq!(rows[0].name, "Applications");
        assert_eq!(rows[1].name, "Mission Center");
        assert_eq!(rows[0].process_pid, None);
        assert_eq!(rows[1].process_pid, None);
        assert_eq!(rows[0].cell_text.pid, "");
        assert_eq!(rows[1].cell_text.pid, "");
        assert_eq!(rows[1].cpu, Some(24.0));
        assert_eq!(rows[1].mem, Some(2_400));
        assert_eq!(rows[1].depth, 1);

        let process_rows = &rows[2..];
        assert_eq!(
            process_rows
                .iter()
                .map(|row| row.process_pid)
                .collect::<Vec<_>>(),
            [Some(100), Some(101), Some(102), Some(103), Some(104)]
        );
        assert_eq!(
            process_rows.iter().map(|row| row.depth).collect::<Vec<_>>(),
            [2, 3, 3, 4, 5]
        );
        assert_eq!(process_rows[0].cpu, Some(10.0));
        assert_eq!(process_rows[0].mem, Some(1_000));
        assert_eq!(process_rows[2].cpu, Some(3.0));
        assert_eq!(
            process_rows[2].mem,
            Some(300),
            "bwrap keeps its own memory, not bwrap + glycin-svg"
        );
        assert_eq!(process_rows[4].cpu, Some(5.0));
        assert_eq!(process_rows[4].cell_text.pid, "104");
        i18n::set_language(prior);
    }

    /// A single-member category still keeps the first-level category header;
    /// the canonical hierarchy must not collapse its root based on cardinality.
    #[test]
    fn single_member_bucket_renders_an_inert_header() {
        let procs = [app_item(11, "solo", 1.0, 100)];
        let refs: Vec<&ProcessItem> = procs.iter().collect();
        let rows = category_tree_rows(&refs, SortCol::Name, true, &HashSet::new(), &HashSet::new());
        assert_eq!(rows.len(), 1);
        assert!(rows[0].has_children);
        assert!(matches!(rows[0].toggle, Toggle::GroupCategory(_)));
        assert_eq!(rows[0].badge, None);
    }

    /// The shared `visible_rows` entry is the category projection (same
    /// headers) and fills the memoized cell text.
    #[test]
    fn visible_rows_uses_canonical_category_projection_and_fills_cell_text() {
        let prior = pinned_english();
        let procs = [
            app_item(11, "editor", 4.0, 400),
            background_item(21, "daemon", 2.5, 250),
            unknown_item(31, "mystery", 0.5, 50),
        ];
        let refs: Vec<&ProcessItem> = procs.iter().collect();
        let rows = visible_rows(VisibleRowsProps {
            processes: &refs,
            query: "",
            sort_col: SortCol::Cpu,
            sort_asc: false,
            filter: ProcessStatusFilter::All,
            collapsed: &HashSet::new(),
            expanded_apps: &HashSet::new(),
        });
        let direct =
            category_tree_rows(&refs, SortCol::Cpu, false, &HashSet::new(), &HashSet::new());
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        let direct_names: Vec<&str> = direct.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, direct_names);
        assert_eq!(
            names,
            ["Applications", "Background processes", "Uncategorized"]
        );
        assert!(
            !rows[0].cell_text.cpu.is_empty(),
            "the dispatch path must fill the memoized cell text"
        );
        i18n::set_language(prior);
    }

    /// Aggregate facts stay raw for sorting/diagnostics, while the displayed
    /// aggregate is the sum of the already-rounded child cells. This catches
    /// the screenshot bug where `12.5%` did not equal `9.1% + 3.3%`, and where
    /// the memory header differed from the visible MB values by one unit.
    #[test]
    fn aggregate_cell_text_is_additive_with_visible_member_text() {
        let prior = pinned_english();
        let mut procs = [
            app_item(11, "editor-main", 9.14, 95_000_000),
            app_item(12, "editor-helper", 3.34, 324_000_000),
            app_item(13, "editor-worker", 0.04, 2_000_000),
            app_item(14, "editor-index", 0.04, 41_000_000),
            app_item(15, "editor-render", 0.04, 79_000_000),
        ];
        for process in &mut procs[1..] {
            process.parent_pid = Some(11);
        }
        let refs: Vec<&ProcessItem> = procs.iter().collect();
        let expanded = HashSet::from([
            category_expansion_key(ProcessCategory::Application),
            "app-tree:11".to_owned(),
        ]);
        let rows = visible_rows(VisibleRowsProps {
            processes: &refs,
            query: "",
            sort_col: SortCol::Cpu,
            sort_asc: false,
            filter: ProcessStatusFilter::All,
            collapsed: &HashSet::new(),
            expanded_apps: &expanded,
        });

        assert_eq!(
            rows.len(),
            7,
            "category header + app aggregate + five members"
        );
        assert_eq!(rows[0].cell_text.cpu, "12.4%");
        assert_eq!(rows[0].cell_text.memory, "541 MB");
        assert_eq!(rows[1].name, "Editor");
        assert_eq!(rows[1].process_pid, None);
        assert_eq!(rows[1].cell_text.pid, "");
        assert_eq!(rows[1].cell_text.cpu, "12.4%");
        assert_eq!(rows[1].cell_text.memory, "541 MB");
        assert_eq!(
            rows[2..]
                .iter()
                .map(|row| row.cell_text.cpu.as_str())
                .collect::<Vec<_>>(),
            vec!["9.1%", "3.3%", "0.0%", "0.0%", "0.0%"]
        );
        assert_eq!(
            rows[2..]
                .iter()
                .map(|row| row.cell_text.memory.as_str())
                .collect::<Vec<_>>(),
            vec!["95 MB", "324 MB", "2 MB", "41 MB", "79 MB"]
        );
        // The raw aggregate remains the exact sum used by sorting and
        // diagnostics; only its presentation follows the visible rounding.
        assert!((rows[0].cpu.unwrap() - 12.60).abs() < f32::EPSILON);
        assert!((rows[1].cpu.unwrap() - 12.60).abs() < f32::EPSILON);
        assert_eq!(rows[0].mem, Some(541_000_000));
        assert_eq!(rows[1].mem, Some(541_000_000));
        i18n::set_language(prior);
    }

    /// The bare Left/Right tree-navigation fold (iced parity) is pure, so the
    /// whole matrix is pinned without a window: Left collapses an expanded
    /// row, Right expands a collapsed one, Left on an already-collapsed row
    /// climbs to the nearest visible selectable ancestor, and Right on an
    /// expanded row (or Left with no selectable ancestor) is an honest no-op
    /// — never a fall-through into column stepping.
    #[test]
    fn structural_arrow_matrix_matches_the_iced_contract() {
        use crate::gpui_app::processes_view::rows::projection::{
            StructuralArrow, structural_arrow_action,
        };
        let parent = Some(ProcessRowKey::Process(100));

        assert_eq!(
            structural_arrow_action(false, parent, false),
            Some(StructuralArrow::Collapse),
            "Left on an expanded row collapses its subtree"
        );
        assert_eq!(
            structural_arrow_action(true, parent, true),
            Some(StructuralArrow::Expand),
            "Right on a collapsed row expands its subtree"
        );
        assert_eq!(
            structural_arrow_action(true, parent, false),
            Some(StructuralArrow::GotoParent(ProcessRowKey::Process(100))),
            "Left on an already-collapsed row climbs to the parent"
        );
        assert_eq!(
            structural_arrow_action(true, None, false),
            None,
            "Left with no selectable ancestor (category/aggregate boundary) is a no-op"
        );
        assert_eq!(
            structural_arrow_action(false, parent, true),
            None,
            "Right on an already-expanded row is a no-op"
        );
    }

    /// Every projected row carries its nearest VISIBLE selectable ancestor:
    /// an app-root process row points at the aggregate above it, in-tree
    /// children point at their real parent row, and structural rows (category
    /// headers, app aggregates, category-tree roots) carry `None` so climbing
    /// stops honestly at the boundary instead of fabricating a target.
    #[test]
    fn parent_key_pins_the_nearest_visible_selectable_ancestor() {
        let prior = pinned_english();
        let identity =
            ProcessApplicationIdentity::new("org.example.MissionCenter", "Mission Center", None)
                .expect("fixture identity must be non-empty");
        let item = |pid: u32, name: &str, cpu: f32, mem: u64, parent_pid: Option<u32>| {
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(pid)
                .parent_pid(parent_pid)
                .name(name.to_owned())
                .current_cpu_percentage(cpu)
                .current_memory_bytes(mem)
                .status("S".to_owned())
                .application_identity_observation(ProcessMetadataObservation::available(
                    identity.clone(),
                    10,
                ))
                .build()
        };
        let background = |pid: u32, name: &str, parent_pid: Option<u32>| {
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(pid)
                .parent_pid(parent_pid)
                .name(name.to_owned())
                .current_cpu_percentage(1.0)
                .current_memory_bytes(100)
                .status("S".to_owned())
                .application_identity_observation(ProcessMetadataObservation::<
                    ProcessApplicationIdentity,
                >::absent(10))
                .build()
        };
        let apps = [
            item(100, "missioncenter", 10.0, 1_000, None),
            item(101, "missioncenter-magpie", 2.0, 200, Some(100)),
            item(102, "bwrap", 3.0, 300, Some(100)),
            item(103, "glycin-svg", 4.0, 400, Some(102)),
        ];
        let backgrounds = [
            background(200, "syslogd", None),
            background(201, "cron", Some(200)),
        ];
        let refs: Vec<&ProcessItem> = apps.iter().chain(backgrounds.iter()).collect();
        let rows = visible_rows(VisibleRowsProps {
            processes: &refs,
            query: "",
            sort_col: SortCol::Pid,
            sort_asc: true,
            filter: ProcessStatusFilter::All,
            collapsed: &HashSet::new(),
            expanded_apps: &HashSet::from([
                category_expansion_key(ProcessCategory::Application),
                category_expansion_key(ProcessCategory::Background),
                "app-tree:100".to_owned(),
            ]),
        });

        // Applications: header, aggregate, then the fully expanded tree.
        assert_eq!(rows[0].parent_key, None, "category header is structural");
        assert_eq!(
            rows[1].parent_key, None,
            "the app aggregate's only parent is the structural header"
        );
        assert_eq!(
            rows[2].parent_key,
            Some(ProcessRowKey::Application(100)),
            "the root process row climbs to the aggregate row above it"
        );
        assert_eq!(
            rows[3].parent_key,
            Some(ProcessRowKey::Process(100)),
            "in-tree children climb to their real parent row"
        );
        assert_eq!(rows[4].parent_key, Some(ProcessRowKey::Process(100)));
        assert_eq!(
            rows[5].parent_key,
            Some(ProcessRowKey::Process(102)),
            "deeper rows climb one visible level at a time"
        );
        // Background keeps the direct tree: its root's parent is the
        // structural header, so climbing stops there.
        let bg_rows = &rows[6..];
        assert_eq!(bg_rows[0].parent_key, None);
        assert_eq!(bg_rows[1].parent_key, None);
        assert_eq!(
            bg_rows[2].parent_key,
            Some(ProcessRowKey::Process(200)),
            "a category-tree child climbs to its in-tree parent"
        );
        i18n::set_language(prior);
    }
}
