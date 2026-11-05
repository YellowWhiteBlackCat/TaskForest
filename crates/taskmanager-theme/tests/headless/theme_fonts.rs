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

#[test]
fn the_bundled_collection_is_one_face_per_role() {
    // The public collection is the only address for these names: every entry is
    // the face of exactly one role, in role order.
    assert_eq!(
        BUNDLED_FONT_FAMILIES,
        FontRole::ALL.map(FontRole::bundled_family)
    );
    assert_eq!(FontRole::Ui.bundled_family(), FONT_MISANS_VF);
    assert_eq!(FontRole::Mono.bundled_family(), FONT_ROBOTO_MONO);
    assert_ne!(
        FontRole::Ui.bundled_family(),
        FontRole::Mono.bundled_family()
    );
    // Every bundled family resolves to itself through the host catalog, and the
    // fallback ladder never re-lists it (the resolver would otherwise pick its
    // own bundle over an installed family).
    for role in FontRole::ALL {
        assert!(!role.fallback_families().contains(&role.bundled_family()));
        assert_eq!(
            role.fallback_families().first().copied(),
            Some(role.empty_catalog_fallback()),
            "{role:?} must fall back to the head of its own ladder"
        );
    }
}

#[test]
fn font_choice_tokens_round_trip_through_the_shared_codec() {
    let availability =
        FontAvailability::from_installed_families([FONT_MISANS_VF, FONT_ROBOTO_MONO, "Fira Sans"]);
    for (role, choices) in [
        (FontRole::Ui, FontChoice::PICKER_LADDER),
        (FontRole::Mono, FontChoice::PICKER_LADDER),
    ] {
        for choice in choices {
            let token = choice.config_token(role);
            assert_eq!(
                FontChoice::from_config_token(token, role, &availability),
                choice,
                "{choice:?} as {role:?} must survive the round trip through {token:?}"
            );
        }
    }
    // The bundled choice persists the role's own face, so one product's config
    // file names the same family the next product paints.
    assert_eq!(
        FontChoice::Bundled.config_token(FontRole::Ui),
        FONT_MISANS_VF
    );
    assert_eq!(
        FontChoice::Bundled.config_token(FontRole::Mono),
        FONT_ROBOTO_MONO
    );
    assert_eq!(
        FontChoice::System.config_token(FontRole::Ui),
        FONT_TOKEN_SYSTEM
    );
    let custom = FontChoice::Custom("Fira Sans");
    assert_eq!(
        FontChoice::from_config_token(
            custom.config_token(FontRole::Ui),
            FontRole::Ui,
            &availability
        ),
        custom
    );
}

#[test]
fn font_preference_tokens_round_trip_for_both_roles_at_once() {
    let availability =
        FontAvailability::from_installed_families([FONT_MISANS_VF, FONT_ROBOTO_MONO, "Fira Sans"]);
    let preference = FontPreference {
        ui: FontChoice::Custom("Fira Sans"),
        mono: FontChoice::System,
    };
    let (ui_token, mono_token) = preference.config_tokens();
    assert_eq!((ui_token, mono_token), ("Fira Sans", FONT_TOKEN_SYSTEM));
    assert_eq!(
        FontPreference::from_config_tokens(ui_token, mono_token, &availability),
        preference
    );
    // The default preference (never chosen) persists as the bundled faces and
    // reads back identically.
    assert_eq!(
        FontPreference::from_config_tokens("", "", &availability),
        FontPreference::default()
    );
    assert_eq!(
        FontPreference::default().config_tokens(),
        (FONT_MISANS_VF, FONT_ROBOTO_MONO)
    );
}

#[test]
fn an_unobserved_family_never_becomes_a_primary_choice() {
    let availability =
        FontAvailability::from_installed_families([FONT_MISANS_VF, FONT_ROBOTO_MONO]);
    // A hand-edited config naming a family the host does not have fails closed to
    // the skin's verified fallback rather than requesting a missing face.
    for token in ["Completely Made Up", "Comic Sans"] {
        assert_eq!(
            FontChoice::from_config_token(token, FontRole::Ui, &availability),
            FontChoice::System
        );
    }
    // The UI face is never silently accepted as the monospace face: the mono role
    // keeps its aligned-digit requirement and falls back to the skin's mono family.
    assert_eq!(
        FontChoice::from_config_token(FONT_MISANS_VF, FontRole::Mono, &availability),
        FontChoice::System
    );
    // …while the reverse spelling a legacy config carried still means bundled.
    assert_eq!(
        FontChoice::from_config_token(FONT_ROBOTO_MONO, FontRole::Ui, &availability),
        FontChoice::Bundled
    );
}

#[test]
fn an_embedded_only_renderer_reports_no_host_catalog() {
    // Bevy registers the embedded faces and cannot load a family by name, so its
    // availability snapshot must not claim a skin family it cannot paint.
    let availability = FontAvailability::embedded_only(true);
    assert!(availability.embedded_fonts_ready());
    assert!(availability.families().is_empty());
    assert!(availability.custom_families().is_empty());
    for skin in Skin::ALL {
        assert_eq!(
            availability.resolved_for(skin),
            ResolvedFonts {
                ui: FONT_MISANS_VF,
                mono: FONT_ROBOTO_MONO,
            }
        );
    }
    // A custom token cannot authorize itself without a catalog, and the bundled
    // policy still resolves the faces this renderer actually holds.
    assert_eq!(
        FontChoice::from_config_token("Fira Sans", FontRole::Ui, &availability),
        FontChoice::System
    );
    assert_eq!(
        resolve_fonts(
            FontPreference::from_config_tokens("Fira Sans", "system", &availability),
            Skin::Windows,
            &availability,
        ),
        ResolvedFonts {
            ui: FONT_MISANS_VF,
            mono: FONT_ROBOTO_MONO,
        },
        "System resolves to the skin slot, which an embedded-only host fills with the bundled faces"
    );
    // When registration failed, the same snapshot reports it honestly.
    let missing = FontAvailability::embedded_only(false);
    assert!(!missing.embedded_fonts_ready());
    assert_eq!(
        missing.resolved_for(Skin::Kde),
        ResolvedFonts {
            ui: FontRole::Ui.empty_catalog_fallback(),
            mono: FontRole::Mono.empty_catalog_fallback(),
        }
    );
}
