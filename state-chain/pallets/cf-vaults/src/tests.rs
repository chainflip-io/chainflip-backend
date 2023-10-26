#![cfg(test)]

use core::panic;

use crate::{
	mock::*, CeremonyId, Error, Event as PalletEvent, KeyHandoverResolutionPendingSince,
	KeygenFailureVoters, KeygenOutcomeFor, KeygenResolutionPendingSince, KeygenResponseTimeout,
	KeygenSuccessVoters, PalletOffence, PendingVaultRotation, Vault, VaultRotationStatus, Vaults,
};
use cf_chains::{
	btc::BitcoinCrypto,
	evm::EvmCrypto,
	mocks::{MockAggKey, MockOptimisticActivation},
};
use cf_primitives::{AuthorityCount, GENESIS_EPOCH};
use cf_test_utilities::{last_event, maybe_last_event};
use cf_traits::{
	mocks::threshold_signer::{MockThresholdSigner, VerificationParams},
	AccountRoleRegistry, AsyncResult, Chainflip, EpochInfo, KeyProvider, SafeMode, SetSafeMode,
	VaultRotator, VaultStatus,
};
use frame_support::{
	assert_noop, assert_ok, pallet_prelude::DispatchResultWithPostInfo, traits::Hooks,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::Get;
use sp_std::collections::btree_set::BTreeSet;

pub type EthMockThresholdSigner = MockThresholdSigner<EvmCrypto, crate::mock::RuntimeCall>;
pub type BtcMockThresholdSigner = MockThresholdSigner<BitcoinCrypto, crate::mock::RuntimeCall>;

const ALL_CANDIDATES: &[<Test as Chainflip>::ValidatorId] = &[ALICE, BOB, CHARLIE];

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
			NEW_AGG_PUB_KEY_POST_HANDOVER,
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
		PendingVaultRotation::put(VaultRotationStatus::<Test, _>::KeyHandoverComplete {
			new_public_key: NEW_AGG_PUB_KEY_POST_HANDOVER,
		});

		VaultsPallet::activate();

		assert_last_event!(crate::Event::AwaitingGovernanceActivation { .. });

		// we're awaiting the governance action, so we are pending from
		// perspective of an outside observer (e.g. the validator pallet)
		assert_eq!(VaultsPallet::status(), AsyncResult::Pending);
	});
}

#[test]
fn when_set_agg_key_with_agg_key_not_required_we_skip_to_completion() {
	new_test_ext().execute_with(|| {
		PendingVaultRotation::put(VaultRotationStatus::<Test, _>::KeyHandoverComplete {
			new_public_key: NEW_AGG_PUB_KEY_POST_HANDOVER,
		});

		MockSetAggKeyWithAggKey::set_required(false);

		VaultsPallet::activate();

		assert!(matches!(
			PendingVaultRotation::<Test, _>::get().unwrap(),
			VaultRotationStatus::Complete
		))
	});
}
