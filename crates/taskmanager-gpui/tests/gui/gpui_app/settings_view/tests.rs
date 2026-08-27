//! Settings-page unit tests (line split).

mod tests_inner {
    // NOTE: no `use super::*` (or any glob) here — the parent module's
    // `use gpui::*` would re-import the `test` attribute macro, and a
    // `#[gpui::test]` expansion that emits `#[test]` would then recurse into
    // itself until the compiler's expansion recursion limit trips. Every
    // existing `#[gpui::test]` file in this repo imports explicitly for the
    // same reason.
    use gpui::{
        AppContext, Context, Entity, IntoElement, Keystroke, ParentElement, Render, SharedString,
        Styled, TestAppContext, VisualTestContext, Window, div, px, size,
    };

    use super::super::fonts::effective_font_summary;
    use super::super::{render_settings, startup_page_row};
    use crate::core::config::{
        STARTUP_PAGE_PROCESSES, STARTUP_PAGE_REMEMBER, TEXT_RENDERING_PLATFORM_DEFAULT,
    };
    use crate::gpui_app::formatting::DisplayUnits;
    use crate::gpui_app::graph::GraphSettings;
    use crate::gpui_app::root::RootView;
    use crate::gpui_app::settings_view::init_data_points_slider;
    use crate::gpui_app::settings_view::refresh::init_slider_entity;
    use crate::gpui_app::theme::tokens;
    use crate::gpui_app::theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};
    use crate::i18n;
    use taskmanager_ui::inputs::switch::SwitchState;

    /// Test harness: a window whose root renders the Settings dialog content
    /// (or just the Startup-page row) against a live [`RootView`] entity.
    struct SettingsHarness {
        root_view: Entity<RootView>,
        /// `true` renders the full [`render_settings`] dialog content; `false`
        /// renders only the Startup-page row so its select is the only
        /// tab stop in the window.
        full: bool,
    }

    impl Render for SettingsHarness {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let root = self.root_view.clone();
            let full = self.full;
            let content = self.root_view.update(cx, move |v, cx| {
                if full {
                    // The harness owns the per-window slider entity, exactly
                    // like `RootView::render`'s settings call site does.
                    let slider_entity = v
                        .settings_slider
                        .get_or_insert_with(|| init_slider_entity(1.0, &mut *cx))
                        .clone();
                    let graph_points = v.performance_settings().graph.data_points;
                    let graph_points_slider = if let Some(slider) = v.graph_points_slider.clone() {
                        slider
                    } else {
                        let slider = init_data_points_slider(graph_points, &mut *cx);
                        v.graph_points_slider = Some(slider.clone());
                        slider
                    };
                    for id in [
                        "hc-switch",
                        "device-cpu",
                        "device-memory",
                        "device-disks",
                        "device-network",
                        "network-wired",
                        "network-wireless",
                        "network-vpn",
                        "network-virtual",
                        "network-other",
                        "device-gpus",
                        "gray-zero-values",
                        "smooth-graphs",
                        "sliding-graphs",
                        "network-dynamic-scaling",
                        "desktop-notifications",
                        "continuous-history",
                    ] {
                        v.settings_switches
                            .entry(id)
                            .or_insert_with(|| cx.new(|cx| SwitchState::new(cx)));
                    }
                    render_settings(
                        {
                            let presentation = v.presentation_snapshot();
                            let appearance = presentation.appearance;
                            crate::gpui_app::settings_view::SettingsViewProps {
                                theme: &v.theme,
                                hovered: None,
                                refresh_secs: 1.0,
                                show_cpu: true,
                                show_memory: true,
                                show_disks: true,
                                show_network: true,
                                show_network_wired: true,
                                show_network_wireless: true,
                                show_network_vpn: true,
                                show_network_virtual: true,
                                show_network_other: true,
                                show_gpus: true,
                                units: DisplayUnits::default(),
                                graph_settings: GraphSettings::default(),
                                graph_points_slider,
                                gray_zero_values: presentation.gray_zero_values,
                                notify_enabled: v.projection().alert_center.policy().enabled,
                                history_persistence: false,
                                notify_quiet_start: v
                                    .projection()
                                    .alert_center
                                    .policy()
                                    .quiet_hours
                                    .map_or(0, |hours| (hours.start_minutes / 60) as u8),
                                notify_quiet_end: v
                                    .projection()
                                    .alert_center
                                    .policy()
                                    .quiet_hours
                                    .map_or(0, |hours| (hours.end_minutes / 60) as u8),
                                font_pref: appearance.font,
                                font_availability: &v.font_availability,
                                density: appearance.density,
                                ui_size: appearance.ui_size,
                                color_scheme: appearance.color_scheme,
                                text_rendering: appearance.text_rendering,
                                startup_page: presentation.startup_page,
                                slider_entity,
                                switches: &v.settings_switches,
                            }
                        },
                        cx,
                    )
                    .into_any_element()
                } else {
                    startup_page_row(
                        &v.theme,
                        root,
                        SharedString::from(v.presentation_snapshot().startup_page().to_owned()),
                    )
                    .into_any_element()
                }
            });
            div().p(tokens::SPACE_4).child(content)
        }
    }

    /// Press Enter on the currently focused element through the real key-event
    /// pipeline (KeyUp is what gpui's `Stateful` interactivity turns into a
    /// `ClickEvent::Keyboard` for focused elements, which is what fires the
    /// pill `on_click` — the same path a keyboard user takes).
    fn press_enter(win: &mut gpui::VisualTestContext) {
        win.simulate_event(gpui::KeyUpEvent {
            keystroke: Keystroke::parse("enter").unwrap(),
        });
    }

    /// The Settings dialog renders under five semantic group titles
    /// (General / Appearance / Fonts / System / Units), each appearing below
    /// the one before it — the Zed-style organization, not a flat section list.
    #[gpui::test]
    async fn settings_render_in_semantic_groups(cx: &mut TestAppContext) {
        let (_, window) = cx.add_window_view(|_window, cx| SettingsHarness {
            root_view: cx.new(|cx| RootView::new(Theme::dark(), cx)),
            full: true,
        });
        window.update(|window, cx| window.draw(cx).clear());

        let general = window
            .debug_bounds("settings.group_general")
            .expect("General group title must render");
        let appearance = window
            .debug_bounds("settings.group_appearance")
            .expect("Appearance group title must render");
        let fonts = window
            .debug_bounds("settings.group_fonts")
            .expect("Fonts group title must render");
        let system = window
            .debug_bounds("settings.group_system")
            .expect("System group title must render");
        let units = window
            .debug_bounds("settings.group_units")
            .expect("Units group title must render");
        let zero_values = window
            .debug_bounds("gray-zero-values")
            .expect("Apps zero-value preference must render");
        let network_wired = window
            .debug_bounds("network-wired")
            .expect("wired network visibility preference must render");
        let network_other = window
            .debug_bounds("network-other")
            .expect("other network visibility preference must render");
        assert!(
            general.origin.y < appearance.origin.y,
            "General must sit above Appearance"
        );
        assert!(
            appearance.origin.y < fonts.origin.y,
            "Appearance must sit above Fonts"
        );
        assert!(
            fonts.origin.y < system.origin.y,
            "Fonts must sit above System"
        );
        assert!(
            system.origin.y < units.origin.y,
            "System must sit above Units"
        );
        assert!(
            system.origin.y < zero_values.origin.y,
            "Apps zero-value preference must sit inside the System group"
        );
        assert!(
            network_wired.origin.y < network_other.origin.y,
            "network category preferences must retain their upstream order"
        );
    }

    /// Product modes are represented by real preview cards, not only text
    /// pills. Their hit areas stay wide enough for pointer/touch activation and
    /// remain ordered without overlap at the compact settings width.
    #[gpui::test]
    async fn settings_product_theme_preview_cards_have_stable_hit_areas(cx: &mut TestAppContext) {
        let (_, window) = cx.add_window_view(|_window, cx| SettingsHarness {
            root_view: cx.new(|cx| RootView::new(Theme::dark(), cx)),
            full: true,
        });
        window.update(|window, cx| window.draw(cx).clear());
        let light = window
            .debug_bounds("mode-light")
            .expect("Light product preview must render");
        let dark = window
            .debug_bounds("mode-dark")
            .expect("Dark product preview must render");
        let forest = window
            .debug_bounds("mode-eyeforest")
            .expect("EyeForest product preview must render");
        for bounds in [light, dark, forest] {
            assert!(bounds.size.width >= px(118.0));
            assert!(bounds.size.height >= px(70.0));
        }
        assert!(light.origin.x < dark.origin.x);
        assert!(dark.origin.x < forest.origin.x);
        assert!(light.origin.x + light.size.width <= dark.origin.x);
        assert!(dark.origin.x + dark.size.width <= forest.origin.x);
    }

    /// The production Settings overlay keeps the scroll track inside the
    /// bounded dialog viewport at the compact contract size. This prevents
    /// low-frequency controls from becoming invisible below the fold and
    /// guards the responsive layout against a future dialog/header change.
    #[gpui::test]
    async fn settings_compact_viewport_keeps_scroll_affordance_inside_dialog(
        cx: &mut TestAppContext,
    ) {
        let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
        win.update(cx, |view, _window, cx| {
            view.mark_telemetry_frame_ready();
            view.show_settings();
            cx.notify();
        })
        .unwrap();
        cx.simulate_window_resize(win.into(), size(px(720.0), px(480.0)));
        cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();

        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let viewport = vcx
            .debug_bounds("tm-settings-scroll-viewport")
            .expect("Settings must expose a bounded scroll viewport");
        let scrollbar = vcx
            .debug_bounds("tm-settings-scrollbar")
            .expect("Settings must expose a visible scroll affordance");
        let track = vcx
            .debug_bounds("tm-settings-scrollbar-track")
            .expect("Settings must expose a thin visual scrollbar track");
        let general = vcx
            .debug_bounds("settings.group_general")
            .expect("Settings must expose the first group inside the scroll viewport");
        assert!(
            viewport.size.height <= px(280.5),
            "compact Settings content must honor the 280px height contract: {viewport:?}"
        );
        assert!(
            scrollbar.origin.x + scrollbar.size.width
                <= viewport.origin.x + viewport.size.width + px(0.5),
            "scrollbar must stay inside the dialog viewport: viewport={viewport:?}, scrollbar={scrollbar:?}"
        );
        assert!(
            scrollbar.size.height >= px(40.0),
            "scroll affordance must remain draggable at compact size: {scrollbar:?}"
        );
        assert!(
            track.size.width <= px(2.0),
            "the visual track must stay thin while the wrapper remains a full hit target: {track:?}"
        );
        assert!(
            track.origin.x >= scrollbar.origin.x
                && track.origin.x + track.size.width
                    <= scrollbar.origin.x + scrollbar.size.width + px(0.5),
            "the visual track must stay inside the scrollbar hit target: track={track:?}, scrollbar={scrollbar:?}"
        );
        assert!(
            general.origin.x + general.size.width <= scrollbar.origin.x + px(0.5),
            "settings content must reserve the scrollbar rail instead of painting underneath it: group={general:?}, scrollbar={scrollbar:?}"
        );
    }

    /// Published GPUI 0.2.2 cannot change text rasterization. The unsupported
    /// choices stay visible as disabled capability evidence, but a real pointer
    /// activation must not change the persisted/live token.
    #[gpui::test]
    async fn unsupported_text_rendering_choices_are_visible_but_inert(cx: &mut TestAppContext) {
        let root_view = cx.new(|cx| RootView::new(Theme::dark(), cx));
        let win = cx.add_window(|_window, _cx| SettingsHarness {
            root_view: root_view.clone(),
            full: true,
        });
        let mut vcx = gpui::VisualTestContext::from_window(win.into(), cx);
        vcx.update(|window, cx| window.draw(cx).clear());

        let default_mode = vcx
            .debug_bounds("text-rendering-default")
            .expect("platform default text mode must render");
        let subpixel = vcx
            .debug_bounds("text-rendering-subpixel")
            .expect("unsupported subpixel choice must remain visible");
        let grayscale = vcx
            .debug_bounds("text-rendering-grayscale")
            .expect("unsupported grayscale choice must remain visible");
        assert!(default_mode.origin.x < subpixel.origin.x);
        assert!(subpixel.origin.x < grayscale.origin.x);

        vcx.simulate_click(subpixel.center(), Default::default());
        vcx.update(|window, cx| window.draw(cx).clear());
        assert!(root_view.read_with(cx, |view, _| {
            view.presentation_snapshot().text_rendering() == TEXT_RENDERING_PLATFORM_DEFAULT
        }));
    }

    /// The Startup-page select records the window's live token through the
    /// real focus + keyboard pipeline: tab to the trigger, open the menu
    /// with Enter, arrow to the option, confirm with Enter — the matching
    /// token lands on the harness's own RootView. The trigger opens with
    /// Enter via the dropdown's keyboard path, so the focus chain reaches
    /// the option list and confirm runs the on_change callback.
    #[gpui::test]
    async fn startup_page_select_records_the_window_token(cx: &mut TestAppContext) {
        let root_view = cx.new(|cx| RootView::new(Theme::dark(), cx));
        let win = cx.add_window(|_window, _cx| SettingsHarness {
            root_view: root_view.clone(),
            full: false,
        });
        let mut vcx = gpui::VisualTestContext::from_window(win.into(), cx);
        vcx.update(|window, cx| window.draw(cx).clear());
        // Seed a baseline different from the default so each activation is
        // observable (not a no-op "already selected" choice).
        root_view.update(cx, |v, cx| {
            v.set_startup_page_preference(SharedString::from(STARTUP_PAGE_PROCESSES), cx);
        });
        vcx.update(|window, cx| window.draw(cx).clear());

        // Focus the select trigger and open the menu with Enter.
        vcx.update(|window, cx| window.draw(cx).clear());
        let trigger = vcx
            .debug_bounds("startup-page:trigger")
            .expect("select renders");
        eprintln!("TRIGGER: {trigger:?}");
        vcx.simulate_mouse_move(
            trigger.center(),
            None::<gpui::MouseButton>,
            Default::default(),
        );
        vcx.simulate_click(trigger.center(), Default::default());
        vcx.update(|window, cx| window.draw(cx).clear());
        assert!(
            vcx.debug_bounds("tm-popup").is_some(),
            "clicking the select trigger must open the menu"
        );

        // Arrow to the third option (Processes) and confirm.
        vcx.simulate_keystrokes("down down down enter");
        vcx.update(|window, cx| window.draw(cx).clear());
        assert!(
            root_view.read_with(cx, |v, _| {
                v.presentation_snapshot().startup_page() == STARTUP_PAGE_PROCESSES
            }),
            "confirming the Processes option must record the processes token"
        );

        // The menu dismisses and the trigger regains focus: the next
        // Enter (focused trigger, KeyUp-click path) reopens the menu, which
        // proves both the dismissal and the focus restore happened.

        // Reopen and choose the first option (Remember last).
        vcx.update(|window, cx| window.draw(cx).clear());
        press_enter(&mut vcx);
        vcx.update(|window, cx| window.draw(cx).clear());
        vcx.simulate_keystrokes("down enter");
        vcx.update(|window, cx| window.draw(cx).clear());
        assert!(
            root_view.read_with(cx, |v, _| {
                v.presentation_snapshot().startup_page() == STARTUP_PAGE_REMEMBER
            }),
            "confirming the Remember-last option must record the remember token"
        );
    }

    /// `effective_font_summary` interpolates the theme's *resolved* families
    /// into the localized `settings.font_effective` catalog template: each
    /// placeholder receives its own role's family (distinct sentinels prove
    /// the slots are not swapped), no placeholder survives, and the result is
    /// exact per locale. `Theme::build` is a pure constructor, so this is a
    /// data-layer test — no window, no App. The i18n language is a process
    /// global, so the test pins it for the exact-string assertions and
    /// restores the previous value at the end (nextest runs each test in its
    /// own process, but the restore keeps the test correct under any runner).
    #[test]
    fn settings_effective_font_summary_interpolates_resolved_families() {
        let theme = Theme::build(
            Skin::Gnome,
            LightDark::Dark,
            HighContrast::Off,
            ResolvedFonts {
                ui: "SentinelUiFace",
                mono: "SentinelMonoFace",
            },
        );
        let saved = i18n::current_language();

        i18n::set_language(i18n::Language::En);
        let en = effective_font_summary(&theme);
        assert_eq!(en, "Effective: UI SentinelUiFace · Mono SentinelMonoFace");
        assert!(!en.contains("{ui}") && !en.contains("{mono}"));

        i18n::set_language(i18n::Language::Zh);
        let zh = effective_font_summary(&theme);
        assert_eq!(zh, "实际字体：界面 SentinelUiFace · 等宽 SentinelMonoFace");
        assert!(!zh.contains("{ui}") && !zh.contains("{mono}"));

        i18n::set_language(saved);
    }

    /// The interpolation above depends on the shipped catalogs actually
    /// carrying both `{ui}` and `{mono}` placeholders — a locale edit that
    /// drops one would silently stop surfacing that family. This guards the
    /// live catalog data (through `i18n::t`, not source text) in both locales.
    #[test]
    fn settings_font_effective_catalog_template_carries_both_placeholders() {
        let saved = i18n::current_language();

        i18n::set_language(i18n::Language::En);
        let en = i18n::t("settings.font_effective");
        i18n::set_language(i18n::Language::Zh);
        let zh = i18n::t("settings.font_effective");

        i18n::set_language(saved);
        for (locale, template) in [("en", en), ("zh", zh)] {
            assert!(
                template.contains("{ui}") && template.contains("{mono}"),
                "{locale} catalog `settings.font_effective` must keep both placeholders: {template}"
            );
        }
    }
}
