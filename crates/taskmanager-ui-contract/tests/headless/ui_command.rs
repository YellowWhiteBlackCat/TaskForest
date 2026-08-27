use super::*;

#[test]
fn every_application_command_has_shared_semantic_presentation() {
    for command in CommandId::ALL {
        let descriptor = descriptor(command);
        assert!(matches!(descriptor.label, MessageKey::CommandLabel(_)));
        assert!(matches!(
            descriptor.description,
            MessageKey::CommandDescription(_)
        ));
        assert!(descriptor.icon.is_some());
    }
}
