use super::*;

#[test]
fn skin_font_families_are_distinct_per_skin() {
    let mut ui: Vec<_> = Skin::ALL.into_iter().map(Skin::ui_font).collect();
    let mut mono: Vec<_> = Skin::ALL.into_iter().map(Skin::mono_font).collect();
    ui.sort_unstable();
    mono.sort_unstable();
    ui.dedup();
    mono.dedup();
    // Every skin names a different system face (the resolver relies on
    // these being the platform-idiomatic families).
    assert_eq!(ui.len(), Skin::ALL.len());
    assert_eq!(mono.len(), Skin::ALL.len());
}

#[test]
fn resolution_defaults_to_bundled_and_system_when_explicit() {
    // The default preference is bundled-first: UI resolves to MiSans VF
    // and mono to Roboto Mono when both registrations are observed,
    // independent of skin.
    let full = FontAvailability::from_installed_families(
        Skin::ALL
            .into_iter()
            .flat_map(|skin| [skin.ui_font(), skin.mono_font()])
            .chain([FONT_MISANS_VF, FONT_ROBOTO_MONO]),
    );
    let pref = FontPreference::default();
    for skin in Skin::ALL {
        let resolved = resolve_fonts(pref, skin, &full);
        assert_eq!(resolved.ui, FONT_MISANS_VF);
        assert_eq!(resolved.mono, FONT_ROBOTO_MONO);
    }
    assert!(full.embedded_fonts_ready());

    // Explicit System: the per-skin family wins when installed…
    let system = FontPreference {
        ui: FontChoice::System,
        mono: FontChoice::System,
    };
    assert_eq!(resolve_fonts(system, Skin::Gnome, &full).ui, "Adwaita Sans");
    assert_eq!(
        resolve_fonts(system, Skin::Windows, &full).ui,
        "Segoe UI Variable"
    );
    assert_eq!(
        resolve_fonts(system, Skin::Gnome, &full).mono,
        "Adwaita Mono"
    );

    // …and the bundled faces take over on a host WITHOUT the skin's fonts
    // so text still renders (and CJK glyphs exist).
    let bare = FontAvailability::from_installed_families(["Noto Sans", "Noto Sans Mono"]);
    let resolved = resolve_fonts(system, Skin::Windows, &bare);
    assert_eq!(resolved.ui, "Noto Sans");
    assert_eq!(resolved.mono, "Noto Sans Mono");

    // Explicit Bundled resolves the same bundled stack as the default.
    let bundled = FontPreference {
        ui: FontChoice::Bundled,
        mono: FontChoice::Bundled,
    };
    assert_eq!(
        resolve_fonts(bundled, Skin::Gnome, &full).ui,
        FONT_MISANS_VF
    );
    assert_eq!(
        resolve_fonts(bundled, Skin::Gnome, &full).mono,
        FONT_ROBOTO_MONO
    );

    // Mixed per-role: system UI + bundled mono.
    let mixed = FontPreference {
        ui: FontChoice::System,
        mono: FontChoice::Bundled,
    };
    assert_eq!(resolve_fonts(mixed, Skin::Kde, &full).ui, "Noto Sans");
    assert_eq!(
        resolve_fonts(mixed, Skin::Kde, &full).mono,
        FONT_ROBOTO_MONO
    );
}

#[test]
fn missing_embedded_face_is_reported_and_does_not_become_a_fake_primary() {
    let only_ui =
        FontAvailability::from_installed_families([FONT_MISANS_VF, "Segoe UI", "Noto Sans Mono"]);
    assert!(!only_ui.embedded_fonts_ready());
    assert!(only_ui.bundled_ui_available());
    assert!(!only_ui.bundled_mono_available());
    assert_eq!(
        resolve_fonts(FontPreference::default(), Skin::Windows, &only_ui).ui,
        FONT_MISANS_VF
    );
    assert_eq!(
        resolve_fonts(FontPreference::default(), Skin::Windows, &only_ui).mono,
        "Noto Sans Mono"
    );
}

#[test]
fn catalog_trims_deduplicates_and_excludes_bundled_faces_from_custom_choices() {
    let availability = FontAvailability::from_installed_families([
        " Fira Sans ",
        "fira sans",
        FONT_MISANS_VF,
        FONT_ROBOTO_MONO,
    ]);
    assert_eq!(availability.custom_families(), ["Fira Sans"]);
    assert_eq!(
        availability.choice_for(" FIRA SANS "),
        Some(FontChoice::Custom("Fira Sans"))
    );
    assert_eq!(availability.choice_for(FONT_MISANS_VF), None);
    assert_eq!(
        resolve_fonts(
            FontPreference {
                ui: FontChoice::Custom(" fira sans "),
                mono: FontChoice::Bundled,
            },
            Skin::Windows,
            &availability,
        )
        .ui,
        "Fira Sans"
    );
}
