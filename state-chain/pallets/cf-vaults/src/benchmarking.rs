//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;
// use sp_std::vec;

#[allow(unused)]
use crate::Pallet;

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
		let chain_id = ChainId::Ethereum;
		let current_block: T::BlockNumber = (0 as u32).into();
			KeygenResolutionPending::<T>::append((
				chain_id,
				current_block
		));
		let caller: T::AccountId = whitelisted_caller();
		let ceremony_id = 1;
		let new_public_key: [u8; 33] = [0x02; 33];

		let mut candidates: BTreeSet<T::ValidatorId> = BTreeSet::new();
		let mut blamed: BTreeSet<T::ValidatorId> = BTreeSet::new();

		for i in 0..150 {
			let validator_id = account("doogle", i, 0);
			blamed.insert(validator_id);
		}

		for i in 0..150 {
			let validator_id = account("doogle", i, 0);
			candidates.insert(validator_id);
		}

		candidates.insert(caller.clone().into());

		let mut keygen_response_status = KeygenResponseStatus::<T>::new(candidates);

		for i in 0..120 {
			let validator_id = account("doogle", i, 0);
			keygen_response_status.add_failure_vote(&validator_id, blamed.clone());
		}

		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingKeygen {  keygen_ceremony_id: ceremony_id, response_status: keygen_response_status},
		);
	} : {
		Pallet::<T>::on_initialize((5 as u32).into());
	}
	on_initialize_success {
		let chain_id = ChainId::Ethereum;
		let current_block: T::BlockNumber = (0 as u32).into();
			KeygenResolutionPending::<T>::append((
				chain_id,
				current_block
		));
		let caller: T::AccountId = whitelisted_caller();
		let ceremony_id = 1;
		let new_public_key: [u8; 33] = [0x02; 33];

		let mut candidates: BTreeSet<T::ValidatorId> = BTreeSet::new();

		for i in 0..150 {
			let validator_id = account("doogle", i, 0);
			candidates.insert(validator_id);
		}

		candidates.insert(caller.clone().into());

		let mut keygen_response_status = KeygenResponseStatus::<T>::new(candidates);

		for i in 0..120 {
			let validator_id = account("doogle", i, 0);
			keygen_response_status.add_success_vote(&validator_id, new_public_key.to_vec());
		}

		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingKeygen {  keygen_ceremony_id: ceremony_id, response_status: keygen_response_status},
		);
	} : {
		Pallet::<T>::on_initialize((5 as u32).into());
	}
	on_initialize_none {
		let chain_id = ChainId::Ethereum;
		let current_block: T::BlockNumber = (0 as u32).into();
		KeygenResolutionPending::<T>::append((
			chain_id,
			current_block
		));
		let caller: T::AccountId = whitelisted_caller();
		let ceremony_id = 1;
		let new_public_key: [u8; 33] = [0x02; 33];

		let mut candidates: BTreeSet<T::ValidatorId> = BTreeSet::new();

		for i in 0..150 {
			let validator_id = account("doogle", i, 0);
			candidates.insert(validator_id);
		}

		candidates.insert(caller.clone().into());

		let keygen_response_status = KeygenResponseStatus::<T>::new(candidates);

		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingKeygen {  keygen_ceremony_id: ceremony_id, response_status: keygen_response_status},
		);
	} : {
		Pallet::<T>::on_initialize((11 as u32).into());
	}
	report_keygen_outcome {
		let caller: T::AccountId = whitelisted_caller();
		let ceremony_id = 1;
		let chain_id = ChainId::Ethereum;
		let new_public_key: [u8; 33] = [0x02; 33];

		let mut candidates: BTreeSet<T::ValidatorId> = generate_validator_set::<T>(150, caller.clone().into());

		// for i in 0..150 {
		// 	let validator_id = account("doogle", i, 0);
		// 	candidates.insert(validator_id);
		// }

		// candidates.insert(caller.clone().into());

		let keygen_response_status = KeygenResponseStatus::<T>::new(candidates);
		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingKeygen {  keygen_ceremony_id: ceremony_id, response_status: keygen_response_status},
		);
		let reported_outcome = KeygenOutcome::Success(Default::default());
	} : _(RawOrigin::Signed(caller), ceremony_id, chain_id, reported_outcome)
	verify {
		let rotation = PendingVaultRotations::<T>::get(chain_id);
		// TODO: Figure out something to proof that this benchmark reaches the most expensive part
	}
	vault_key_rotated {
		let chain_id = ChainId::Ethereum;
		let caller: T::AccountId = whitelisted_caller();
		let new_public_key: [u8; 33] = [0x02; 33];
		let tx_hash: [u8; 32] = [0xab; 32];

		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingRotation {  new_public_key: new_public_key.to_vec() },
		);
		let call = Call::<T>::vault_key_rotated(chain_id, new_public_key.to_vec(), 5 as u64, tx_hash.to_vec());
		let origin = T::EnsureWitnessed::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::MockRuntime,);
