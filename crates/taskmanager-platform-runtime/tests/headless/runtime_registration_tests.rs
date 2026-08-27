use taskmanager_application::{CapabilityStatus, CommandLaunchRequest, ProviderId};

use super::*;

#[test]
fn provider_mapping_preserves_identity_and_request_type() {
    let registration = ProviderRegistration::<CommandLaunchRequest, _>::new(
        ProviderId::borrowed("fixture.command"),
        7_u8,
    )
    .with_initial_status(CapabilityStatus::PermissionRequired)
    .map_provider(u16::from);

    assert_eq!(
        registration.provider_id(),
        &ProviderId::borrowed("fixture.command")
    );
    assert_eq!(
        registration.binding().as_ref(),
        Some(&ProviderId::borrowed("fixture.command"))
    );
    assert_eq!(
        registration
            .binding()
            .route_parts()
            .map(|(_, status)| status),
        Some(CapabilityStatus::PermissionRequired),
        "provider erasure must preserve composition-time capability health",
    );
    let (provider_id, provider) = registration.into_parts();
    assert_eq!(provider_id, ProviderId::borrowed("fixture.command"));
    assert_eq!(provider, 7_u16);
}
