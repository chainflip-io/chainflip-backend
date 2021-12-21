//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;

#[allow(unused)]
use crate::Pallet;

// Note: Currently we only have one chain (ETH) - as soon we've
// another chain we've to take this in account in our weight calculation benchmark.

const CEREMONY_ID: u64 = 1;
const CHAIN_ID: ChainId = ChainId::Ethereum;
const NEW_PUBLIC_KEY: [u8; 33] = [0x02; 33];
const TX_HASH: [u8; 32] = [0xab; 32];

/// Generate a validator set
fn generate_validator_set<T: Config>(
	amount: u32,
	caller: T::ValidatorId,
) -> BTreeSet<T::ValidatorId> {
	let mut validator_set: BTreeSet<T::ValidatorId> = BTreeSet::new();
	for i in 0..amount {
		let validator_id = account("doogle", i, 0);
		validator_set.insert(validator_id);
	}
	validator_set.insert(caller);
	validator_set
}

benchmarks! {
	on_initialize_failure {
		let current_block: T::BlockNumber = (0 as u32).into();
			KeygenResolutionPending::<T>::append((
				CHAIN_ID,
				current_block
		));
		let caller: T::AccountId = whitelisted_caller();
		let candidates: BTreeSet<T::ValidatorId> = generate_validator_set::<T>(150, caller.clone().into());
		let blamed: BTreeSet<T::ValidatorId> = generate_validator_set::<T>(150, caller.clone().into());
		let mut keygen_response_status = KeygenResponseStatus::<T>::new(candidates);

		for i in 0..120 {
			let validator_id = account("doogle", i, 0);
			let _ = keygen_response_status.add_failure_vote(&validator_id, blamed.clone());
		}

		PendingVaultRotations::<T>::insert(
			CHAIN_ID,
			VaultRotationStatus::<T>::AwaitingKeygen {  keygen_ceremony_id: CEREMONY_ID, response_status: keygen_response_status},
		);
	} : {
		Pallet::<T>::on_initialize((5 as u32).into());
	}
	verify {
		assert!(!PendingVaultRotations::<T>::contains_key(CHAIN_ID));
	}
	on_initialize_success {
		let current_block: T::BlockNumber = (0 as u32).into();
			KeygenResolutionPending::<T>::append((
				CHAIN_ID,
				current_block
		));
		let caller: T::AccountId = whitelisted_caller();
		let candidates: BTreeSet<T::ValidatorId> = generate_validator_set::<T>(150, caller.clone().into());
		let mut keygen_response_status = KeygenResponseStatus::<T>::new(candidates);

		for i in 0..120 {
			let validator_id = account("doogle", i, 0);
			let _ = keygen_response_status.add_success_vote(&validator_id, NEW_PUBLIC_KEY.to_vec());
		}

		PendingVaultRotations::<T>::insert(
			CHAIN_ID,
			VaultRotationStatus::<T>::AwaitingKeygen {  keygen_ceremony_id: CEREMONY_ID, response_status: keygen_response_status},
		);
	} : {
		Pallet::<T>::on_initialize((5 as u32).into());
	}
	verify {
		assert!(PendingVaultRotations::<T>::contains_key(CHAIN_ID));
	}
	on_initialize_none {
		let current_block: T::BlockNumber = (0 as u32).into();
		KeygenResolutionPending::<T>::append((
			CHAIN_ID,
			current_block
		));
		let caller: T::AccountId = whitelisted_caller();
		let candidates: BTreeSet<T::ValidatorId> = generate_validator_set::<T>(150, caller.clone().into());
		let keygen_response_status = KeygenResponseStatus::<T>::new(candidates);

		PendingVaultRotations::<T>::insert(
			CHAIN_ID,
			VaultRotationStatus::<T>::AwaitingKeygen {  keygen_ceremony_id: CEREMONY_ID, response_status: keygen_response_status},
		);
	} : {
		Pallet::<T>::on_initialize((11 as u32).into());
	}
	verify {
		assert_eq!(KeygenResolutionPending::<T>::get().len(), 0);
	}
	report_keygen_outcome {
		let caller: T::AccountId = whitelisted_caller();
		let candidates: BTreeSet<T::ValidatorId> = generate_validator_set::<T>(150, caller.clone().into());
		let keygen_response_status = KeygenResponseStatus::<T>::new(candidates);

		PendingVaultRotations::<T>::insert(
			CHAIN_ID,
			VaultRotationStatus::<T>::AwaitingKeygen {  keygen_ceremony_id: CEREMONY_ID, response_status: keygen_response_status},
		);
		let reported_outcome = KeygenOutcome::Success(Default::default());
	} : _(RawOrigin::Signed(caller), CEREMONY_ID, CHAIN_ID, reported_outcome)
	verify {
		assert_eq!(KeygenResolutionPending::<T>::get().len(), 1);
	}
	vault_key_rotated {
		let caller: T::AccountId = whitelisted_caller();
		PendingVaultRotations::<T>::insert(
			CHAIN_ID,
			VaultRotationStatus::<T>::AwaitingRotation {  new_public_key: NEW_PUBLIC_KEY.to_vec() },
		);
		let call = Call::<T>::vault_key_rotated(CHAIN_ID, NEW_PUBLIC_KEY.to_vec(), 5 as u64, TX_HASH.to_vec());
		let origin = T::EnsureWitnessed::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(Vaults::<T>::contains_key(T::EpochInfo::epoch_index(), ChainId::Ethereum));
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::MockRuntime,);
