#![cfg(test)]

use crate::{mock::*, PendingVaultActivation, VaultActivationStatus, VaultStartBlockNumbers};
use cf_chains::mocks::{MockAggKey, MockEthereum};
use cf_test_utilities::last_event;
use cf_traits::{
	mocks::block_height_provider::BlockHeightProvider, AsyncResult, EpochInfo, SafeMode,
	VaultActivator,
};
use frame_support::assert_ok;
use sp_core::Get;

pub const NEW_AGG_PUBKEY: MockAggKey = MockAggKey(*b"newk");

macro_rules! assert_last_event {
	($pat:pat) => {
		let event = last_event::<Test>();
		assert!(
			matches!(event, $crate::mock::RuntimeEvent::VaultsPallet($pat)),
			"Unexpected event {:?}",
			event
		);
	};
}

#[test]
fn test_vault_key_rotated_externally_triggers_code_red() {
	new_test_ext().execute_with(|| {
		const TX_HASH: [u8; 4] = [0xab; 4];
		assert_eq!(<MockRuntimeSafeMode as Get<MockRuntimeSafeMode>>::get(), SafeMode::CODE_GREEN);
		assert_ok!(VaultsPallet::vault_key_rotated_externally(
			RuntimeOrigin::root(),
			NEW_AGG_PUBKEY,
			1,
			TX_HASH,
		));
		assert_eq!(<MockRuntimeSafeMode as Get<MockRuntimeSafeMode>>::get(), SafeMode::CODE_RED);
		assert_last_event!(crate::Event::VaultRotatedExternally(..));
	});
}

#[test]
fn key_unavailable_on_activate_returns_governance_event() {
	new_test_ext_no_key().execute_with(|| {
		VaultsPallet::start_key_activation(NEW_AGG_PUBKEY, None);

		assert_last_event!(crate::Event::AwaitingGovernanceActivation { .. });

		// we're awaiting the governance action, so we are pending from
		// perspective of an outside observer (e.g. the validator pallet)
		assert_eq!(VaultsPallet::status(), AsyncResult::Pending);
	});
}

#[test]
fn when_set_agg_key_with_agg_key_not_required_we_skip_to_completion() {
	new_test_ext().execute_with(|| {
		MockSetAggKeyWithAggKey::set_required(false);

		VaultsPallet::start_key_activation(NEW_AGG_PUBKEY, Some(Default::default()));

		assert!(matches!(
			PendingVaultActivation::<Test, _>::get().unwrap(),
			VaultActivationStatus::Complete
		))
	});
}

#[test]
fn vault_start_block_number_is_set_correctly() {
	new_test_ext_no_key().execute_with(|| {
		BlockHeightProvider::<MockEthereum>::set_block_height(1000);
		VaultStartBlockNumbers::<Test, _>::insert(MockEpochInfo::epoch_index(), 0);
		VaultsPallet::start_key_activation(NEW_AGG_PUBKEY, Some(Default::default()));
		VaultsPallet::activate_key();
		assert_eq!(
			crate::VaultStartBlockNumbers::<Test, _>::get(
				MockEpochInfo::epoch_index().saturating_add(1)
			)
			.unwrap(),
			1001
		);
		assert!(matches!(
			PendingVaultActivation::<Test, _>::get().unwrap(),
			VaultActivationStatus::Complete
		));
		assert_last_event!(crate::Event::VaultActivationCompleted);
	});
}
