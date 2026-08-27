use taskmanager_platform_contract::ProviderFailure;

use super::assert_identity_change_is_side_effect_free;

#[test]
fn identity_contract_requires_the_typed_failure_and_zero_side_effects() {
    assert!(
        assert_identity_change_is_side_effect_free::<()>(
            &Err(ProviderFailure::IdentityChanged),
            4,
            4,
        )
        .is_ok()
    );
    assert!(assert_identity_change_is_side_effect_free(&Ok(()), 4, 4).is_err());
    assert!(
        assert_identity_change_is_side_effect_free::<()>(
            &Err(ProviderFailure::IdentityChanged),
            4,
            5,
        )
        .is_err()
    );
}
