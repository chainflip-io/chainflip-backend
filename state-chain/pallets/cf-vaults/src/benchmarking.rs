//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::eth::AggKey;
use cf_traits::EpochInfo;
use frame_benchmarking::{
	account, benchmarks_instance_pallet, impl_benchmark_test_suite, whitelisted_caller,
};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;

// Note: Currently we only have one chain (ETH) - as soon we've
// another chain we've to take this in account in our weight calculation benchmark.

const CEREMONY_ID: u64 = 1;
const NEW_PUBLIC_KEY: [u8; 33] = [0x02; 33];
const TX_HASH: [u8; 32] = [0xab; 32];

/// Generate a validator set
fn generate_validator_set<T: Config<I>, I: 'static>(
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

benchmarks_instance_pallet! {
	on_initialize_failure {
		let b in 101 .. 150;
		let current_block: T::BlockNumber = 0u32.into();
		KeygenResolutionPendingSince::<T, I>::put(current_block);
		let caller: T::AccountId = whitelisted_caller();
		let candidates: BTreeSet<T::ValidatorId> = generate_validator_set::<T, I>(150, caller.clone().into());
		let blamed: BTreeSet<T::ValidatorId> = generate_validator_set::<T, I>(b, caller.clone().into());
		let mut keygen_response_status = KeygenResponseStatus::<T, I>::new(candidates);

		for i in 0..b {
			let validator_id = account("doogle", i, 0);
			let _result = keygen_response_status.add_failure_vote(&validator_id, blamed.clone());
		}

		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygen {  keygen_ceremony_id: CEREMONY_ID, response_status: keygen_response_status},
		);
	} : {
		Pallet::<T, I>::on_initialize(5u32.into());
	}
	verify {
		assert!(!PendingVaultRotation::<T, I>::exists());
	}
	on_initialize_success {
		let current_block: T::BlockNumber = 0u32.into();
		KeygenResolutionPendingSince::<T, I>::put(current_block);
		let caller: T::AccountId = whitelisted_caller();
		let candidates: BTreeSet<T::ValidatorId> = generate_validator_set::<T, I>(150, caller.clone().into());
		let mut keygen_response_status = KeygenResponseStatus::<T, I>::new(candidates);

		for i in 0..120 {
			let validator_id = account("doogle", i, 0);
			let _result = keygen_response_status.add_success_vote(&validator_id, AggKey::from_pubkey_compressed(NEW_PUBLIC_KEY));
		}

		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygen {  keygen_ceremony_id: CEREMONY_ID, response_status: keygen_response_status},
		);
	} : {
		Pallet::<T, I>::on_initialize(5u32.into());
	}
	verify {
		assert!(PendingVaultRotation::<T, I>::exists());
	}
	report_keygen_outcome {
		let caller: T::AccountId = whitelisted_caller();
		let candidates: BTreeSet<T::ValidatorId> = generate_validator_set::<T, I>(150, caller.clone().into());
		let keygen_response_status = KeygenResponseStatus::<T, I>::new(candidates);

		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygen { keygen_ceremony_id: CEREMONY_ID, response_status: keygen_response_status},
		);
		let reported_outcome = KeygenOutcomeFor::<T, I>::Success(AggKey::from_pubkey_compressed([0xbb; 33]));
	} : _(RawOrigin::Signed(caller), CEREMONY_ID, reported_outcome)
	verify {
		let rotation = PendingVaultRotation::<T, I>::get().unwrap();
		assert!(matches!(
			rotation,
			VaultRotationStatus::AwaitingKeygen { response_status, .. }
				if response_status.response_count() == 1
		))
	}
	vault_key_rotated {
		let caller: T::AccountId = whitelisted_caller();
		let new_public_key = AggKey::from_pubkey_compressed([0xbb; 33]);
		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingRotation { new_public_key },
		);
		let call = Call::<T, I>::vault_key_rotated(new_public_key, 5u64, Decode::decode(&mut &TX_HASH[..]).unwrap());
		let origin = T::EnsureWitnessed::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(Vaults::<T, I>::contains_key(T::EpochInfo::epoch_index()));
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::MockRuntime,);
