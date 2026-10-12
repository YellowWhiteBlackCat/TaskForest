use super::spec::{COMMAND_SPECS, CommandEnableRule};
use super::*;
use crate::router::{CommandContext, default_bindings, default_router};

fn enabled_context(spec: &super::spec::CommandSpec) -> CommandContext {
    let mut context = CommandContext {
        scope: spec.scope,
        ..CommandContext::default()
    };
    match spec.enable {
        CommandEnableRule::Always => {}
        CommandEnableRule::NotTextInput => {}
        CommandEnableRule::RequiresSelection => context.process_selected = true,
        CommandEnableRule::NoOverlay => {}
        CommandEnableRule::OverlayOnly => context.overlay_present = true,
    }
    context
}

#[test]
fn registry_generates_complete_unique_ids_and_routes_each_typed_action() {
    assert_eq!(CommandId::ALL.len(), COMMAND_SPECS.len());
    assert_eq!(CommandId::ALL.len(), default_bindings().len());

    for (index, command) in CommandId::ALL.iter().enumerate() {
        assert_eq!(
            *command, COMMAND_SPECS[index].id,
            "canonical IDs must stay aligned with their generated metadata rows"
        );
        assert!(
            !CommandId::ALL[index + 1..].contains(command),
            "command IDs must be unique: {command:?}"
        );
    }

    let router = default_router().expect("registered default bindings do not conflict");
    for spec in COMMAND_SPECS {
        let binding = spec.binding();
        let matching_bindings: Vec<_> = default_bindings()
            .iter()
            .filter(|candidate| candidate.command == spec.id)
            .collect();
        assert_eq!(
            matching_bindings.len(),
            1,
            "every registered command has exactly one default binding"
        );
        assert_eq!(
            **matching_bindings.first().expect("binding count is one"),
            binding,
            "every registered command has its declared default binding"
        );

        let context = enabled_context(spec);
        assert_eq!(
            router.resolve_command(binding.chord, context),
            Some(spec.id),
            "the declared chord resolves to its command in an enabled context"
        );
        assert_eq!(
            router.route(binding.chord, context),
            Some(spec.action),
            "the router emits the action declared by the same registry row"
        );
        assert_eq!(spec.id.action(), spec.action);
        assert!(!spec.label_key.is_empty());
        assert!(!spec.description_key.is_empty());
    }
}

#[test]
fn palette_api_discovers_available_commands_and_explains_every_exclusion() {
    let palette: Vec<_> = command_palette_commands().collect();

    for command in CommandId::ALL {
        let discovered = palette.contains(&command);
        match command.palette_disposition() {
            CommandPaletteDisposition::Available => assert!(
                discovered,
                "available command must be discoverable: {command:?}"
            ),
            CommandPaletteDisposition::Excluded(reason) => {
                assert!(
                    !discovered,
                    "excluded command must not be discoverable: {command:?}"
                );
                assert_eq!(
                    command.palette_disposition().exclusion_reason(),
                    Some(reason)
                );
            }
        }
    }

    assert!(palette.contains(&CommandId::ShowPerformance));
    assert!(palette.contains(&CommandId::Refresh));
    assert!(palette.contains(&CommandId::OpenProperties));
    assert!(!palette.contains(&CommandId::EndTask));
    assert_eq!(
        CommandId::EndTask.palette_disposition(),
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::ConfirmationRequired)
    );
    assert_eq!(
        CommandId::PageDown.palette_disposition(),
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::ContextualNavigation)
    );
    assert_eq!(
        CommandId::Confirm.palette_disposition(),
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::OverlayLifecycle)
    );
    assert_eq!(
        CommandId::ShowAlerts.palette_disposition(),
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::FrontendOwnedSurface)
    );
    assert_eq!(
        CommandId::CopySelectedRow.palette_disposition(),
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::SelectionContext)
    );
}

#[test]
fn canonical_key_names_parse_without_driving_routing_by_strings() {
    assert_eq!(KeyCode::parse("PageUp"), Ok(KeyCode::PageUp));
    assert_eq!(KeyCode::parse("F9"), Ok(KeyCode::F9));
    assert_eq!(
        KeyChord::from_key_name("f", Modifiers::CONTROL),
        Ok(KeyChord::new(KeyCode::F, Modifiers::CONTROL))
    );
    assert_eq!(KeyCode::parse("control-f"), Err(KeyParseError::UnknownKey));
}
