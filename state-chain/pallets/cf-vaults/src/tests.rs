#![cfg(test)]

use crate::{
	mock::*, Event, PendingVaultActivation, VaultActivationStatus, VaultStartBlockNumbers,
};
use cf_chains::mocks::{MockAggKey, MockEthereum};
use cf_test_utilities::last_event;
use cf_traits::{
	mocks::block_height_provider::BlockHeightProvider, AsyncResult, EpochInfo,
	EpochTransitionHandler, VaultActivator,
};
use cf_utilities::assert_matches;
use frame_support::assert_noop;
use std::collections::BTreeSet;

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
fn key_unavailable_on_activate_returns_governance_event() {
	new_test_ext_no_key().execute_with(|| {
		VaultsPallet::start_key_activation(NEW_AGG_PUBKEY, None);

		assert_last_event!(Event::AwaitingGovernanceActivation { .. });

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

		assert_matches!(
			PendingVaultActivation::<Test, _>::get().unwrap(),
			VaultActivationStatus::Complete
		);
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
		assert_matches!(
			PendingVaultActivation::<Test, _>::get().unwrap(),
			VaultActivationStatus::Complete
		);
		assert_last_event!(Event::VaultActivationCompleted);
	});
}

#[test]
fn vault_start_block_number_not_set_when_chain_not_initialized() {
	new_test_ext_no_key().execute_with(|| {
		use crate::ChainInitialized;
		BlockHeightProvider::<MockEthereum>::set_block_height(1000);
		ChainInitialized::<Test, _>::put(false);
		VaultsPallet::start_key_activation(NEW_AGG_PUBKEY, Some(Default::default()));
		VaultsPallet::activate_key();
		assert!(VaultStartBlockNumbers::<Test, _>::iter_keys().next().is_none());
		assert_matches!(
			PendingVaultActivation::<Test, _>::get().unwrap(),
			VaultActivationStatus::Complete
		);
	});
}

#[test]
fn only_governance_can_initialize_chain() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			VaultsPallet::initialize_chain(RuntimeOrigin::signed(100)),
			sp_runtime::traits::BadOrigin,
		);
	});
}

#[test]
fn cleanup_start_block_numbers_up_to_expired_epoch() {
	new_test_ext_no_key().execute_with(|| {
		let init_epoch = MockEpochInfo::epoch_index();
		VaultStartBlockNumbers::<Test, _>::insert(init_epoch, 0);
		VaultStartBlockNumbers::<Test, _>::insert(init_epoch.saturating_add(1), 1001);
		VaultStartBlockNumbers::<Test, _>::insert(init_epoch.saturating_add(2), 2001);
		VaultStartBlockNumbers::<Test, _>::insert(init_epoch.saturating_add(3), 3001);

		assert_eq!(
			VaultStartBlockNumbers::<Test, _>::iter_values().collect::<BTreeSet<_>>(),
			BTreeSet::from([0, 1001, 2001, 3001])
		);

		VaultsPallet::on_expired_epoch(init_epoch.saturating_add(2));
		assert_eq!(
			VaultStartBlockNumbers::<Test, _>::iter_values().collect::<BTreeSet<_>>(),
			BTreeSet::from([3001])
		);
	});
}
