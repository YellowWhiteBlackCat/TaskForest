//! test-intent: behavior
//! source-inspection: textual-artifact
//!
//! The visual-parity gates — the tripwires that make the 2026-08-29 capture
//! review failures structurally impossible to re-ship:
//!
//! - the tofu law: this frontend never paints decoration through text
//!   codepoints. Source carries no decoration-range glyph in code position;
//!   icons resolve through the semantic registry into bitmap plates;
//! - the plate registry: every semantic icon this frontend draws decodes to
//!   a correctly-sized bitmap and stamps onto a spawned scene node;
//! - the nav strip: the tab set IS the shared `AppPage::ALL` vocabulary (one
//!   authority — a drifting tab order or a bevy-only tab cannot compile past
//!   this test), and every tab's icon exists in both asset forms;
//! - the bounded-line contract: fact/stat rows compose NoWrap text with a
//!   clipping ancestor, so a long value can never wrap a row into a stack
//!   (the exact defect the capture review caught on the stats rail).
//!
//! Mounted from `lib.rs` (cross-module contract; no single owner module).

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::system::RunSystemOnce;
use bevy::scene::{Scene, ScenePlugin, WorldSceneExt, bsn, template_value};
use std::path::{Path, PathBuf};

use crate::icons::{IconPlates, PLATE_ICONS, icon_rgba, icon_scene};
use crate::palette::{no_wrap_text, ui_palette};
use crate::window::{Role, TextRole};
use bevy::ui::widget::Text;
use taskmanager_theme::Theme;
use taskmanager_ui_contract::IconId;

const PLATE_EDGE_PX: usize = 36;
const PLATE_BYTES: usize = PLATE_EDGE_PX * PLATE_EDGE_PX * 4;

#[test]
fn every_drawn_icon_resolves_to_a_correctly_sized_bitmap() {
    for &icon in PLATE_ICONS {
        let rgba = icon_rgba(icon)
            .unwrap_or_else(|| panic!("{icon:?} has no bitmap plate; regenerate UI icons"));
        assert_eq!(
            rgba.len(),
            PLATE_BYTES,
            "{icon:?} plate is {} bytes, expected {PLATE_BYTES}",
            rgba.len()
        );
    }
}

#[test]
fn the_nav_tab_set_is_the_shared_page_vocabulary_onto_shared_icons() {
    // The strip's tabs, routed back through the shared action vocabulary,
    // must equal AppPage::ALL exactly — same pages, same order. A bevy-only
    // tab or a reordered strip forks the product's route contract.
    let routed: Vec<_> = crate::app::NAV_TABS
        .iter()
        .map(|&page| crate::app::action_for_page(page))
        .collect();
    let shared: Vec<_> = taskmanager_application::AppPage::ALL
        .iter()
        .map(|&page| Some(taskmanager_application::AppAction::SelectPage(page)))
        .collect();
    assert_eq!(routed, shared, "nav tabs must mirror AppPage::ALL in order");
    // Every tab and trailing affordance resolves its icon in BOTH asset
    // forms: the SVG source GPUI draws and the bitmap plate this frontend
    // draws. One semantic identity, two materializations, no drift.
    for &page in crate::app::NAV_TABS
        .iter()
        .chain([crate::app::Page::Alerts, crate::app::Page::Settings].iter())
    {
        let icon = crate::app::tab_icon(page);
        let path = taskmanager_icons::path(icon);
        assert!(
            taskmanager_assets::asset_bytes(path).is_some(),
            "{page:?} icon {path} has no SVG source"
        );
        assert!(
            taskmanager_assets::ui_icon_rgba(path).is_some(),
            "{page:?} icon {path} has no bitmap plate"
        );
    }
}

#[test]
fn icon_plates_build_and_stamp_onto_spawned_scenes() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<bevy::image::Image>>();
    let plates = app
        .world_mut()
        .run_system_once(
            |mut images: bevy::ecs::system::ResMut<Assets<bevy::image::Image>>| {
                IconPlates::build(&mut images)
            },
        )
        .expect("system runs");
    assert_eq!(
        plates.resolved(),
        PLATE_ICONS.len(),
        "every registered icon decodes headlessly"
    );

    // A spawned icon scene carries the plate + ink markers; the stamp
    // observer joins them with the store (handle) and the palette ink.
    app.insert_resource(plates);
    app.add_observer(crate::icons::stamp_icon_plate);
    let ink = bevy::color::Color::srgb(0.2, 0.4, 0.6);
    let scene = icon_scene(IconId::Cpu, 16.0, ink);
    let entity = app
        .world_mut()
        .spawn_scene(scene)
        .expect("scene spawns")
        .id();
    app.world_mut().flush();
    let world = app.world();
    let node = world
        .get::<bevy::ui::widget::ImageNode>(entity)
        .expect("icon scene mounts an ImageNode");
    assert_eq!(node.color, ink, "the plate inherits the theme ink");
    assert_ne!(
        node.image,
        bevy::asset::Handle::default(),
        "the stamp observer resolved the bitmap handle"
    );
}

#[test]
fn source_carries_no_decoration_codepoints_in_code_position() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = manifest.join("src");
    let mut files = Vec::new();
    collect_rust_files(&src, &mut files);
    assert!(files.len() > 20, "the source walk found the crate");

    for file in files {
        let raw = std::fs::read_to_string(&file).expect("readable source");
        let code = strip_comments(&raw);
        for (index, ch) in code.char_indices() {
            if is_decoration_codepoint(ch) {
                let line = raw[..index].lines().count() + 1;
                panic!(
                    "{}:{line}: decoration codepoint U+{:04X} in code position — \
                     icons come from the semantic registry (crate::icons), never \
                     from text glyphs (the tofu law)",
                    file.display(),
                    ch as u32
                );
            }
        }
    }
}

fn is_decoration_codepoint(ch: char) -> bool {
    let c = ch as u32;
    // Arrows, misc technical, enclosed alphanumerics, box/geometric shapes,
    // misc symbols (gears, chess), dingbats, private use, emoji blocks — the
    // exact ranges every earlier tofu glyph lived in.
    (0x2190..=0x21FF).contains(&c)
        || (0x2300..=0x23FF).contains(&c)
        || (0x2460..=0x24FF).contains(&c)
        || (0x2500..=0x25FF).contains(&c)
        || (0x2600..=0x26FF).contains(&c)
        || (0x2700..=0x27BF).contains(&c)
        || (0xE000..=0xF8FF).contains(&c)
        || (0x1F000..=0x1FAFF).contains(&c)
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("readable source dir");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Strip line and block comments so documentation prose (which may quote a
/// forbidden glyph historically) never masks or fakes a violation. Strings
/// are NOT protected: a forbidden glyph inside a string literal is precisely
/// the violation this gate exists to catch.
fn strip_comments(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let mut in_line = false;
    let mut in_block = false;
    while let Some(ch) = chars.next() {
        if in_line {
            if ch == '\n' {
                in_line = false;
                out.push(ch);
            }
            continue;
        }
        if in_block {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block = false;
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            in_line = true;
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block = true;
            continue;
        }
        out.push(ch);
    }
    out
}

#[test]
fn bounded_fact_lines_carry_the_single_line_contract() {
    let palette = ui_palette(&Theme::dark());
    // The rail's key/value row: the value must be NoWrap text under a
    // clipping, shrinkable wrapper — a long fact clips, never wraps. The
    // caller-supplied value scene follows the same bounded-line discipline
    // every page value uses (NoWrap text).
    let long_value = "12.6 GiB / 32.0 GiB - a fact long enough to overflow";
    let value = Box::new(bsn! {
        Text({ long_value })
        TextRole(Role::Mono)
        template_value(no_wrap_text())
    }) as Box<dyn Scene>;
    let row = crate::widgets::controls::stat_row_scene("label".to_owned(), value, &palette);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    let root = app
        .world_mut()
        .spawn_scene(Box::new(row))
        .expect("scene spawns")
        .id();
    app.world_mut().flush();

    let world = app.world();
    let mut texts = 0usize;
    for child in world
        .get::<Children>(root)
        .expect("row mounts children")
        .iter()
    {
        assert_value_column_contract(world, *child, &mut texts);
    }
    assert!(texts >= 2, "the row carries label and value text");
}

/// Walk one column subtree: every Text descendant must be NoWrap, and its
/// column wrapper (the direct child of the row) must clip the x axis.
fn assert_value_column_contract(
    world: &bevy::ecs::world::World,
    column: bevy::ecs::entity::Entity,
    texts: &mut usize,
) {
    let node = world
        .get::<bevy::ui::prelude::Node>(column)
        .expect("column node");
    assert_eq!(
        node.overflow.x,
        bevy::ui::OverflowAxis::Clip,
        "bounded columns clip — a wrap here is the rail-stack defect"
    );
    if let Some(children) = world.get::<Children>(column) {
        for child in children.iter() {
            if let Some(layout) = world.get::<bevy::text::TextLayout>(*child) {
                *texts += 1;
                assert_eq!(
                    layout.linebreak,
                    bevy::text::LineBreak::NoWrap,
                    "bounded lines never wrap"
                );
            }
        }
    }
}
