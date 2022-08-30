//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::EpochInfo;
use codec::{Decode, Encode};
use frame_benchmarking::{account, benchmarks_instance_pallet, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;

// Note: Currently we only have one chain (ETH) - as soon we've
// another chain we've to take this in account in our weight calculation benchmark.

const CEREMONY_ID: u64 = 1;
const NEW_PUBLIC_KEY: [u8; 33] = [0x01; 33];
const TX_HASH: [u8; 32] = [0xab; 32];

/// Generate an authority set
fn generate_authority_set<T: Config<I>, I: 'static>(
	set_size: u32,
	caller: T::ValidatorId,
) -> BTreeSet<T::ValidatorId> {
	let mut authority_set: BTreeSet<T::ValidatorId> = BTreeSet::new();
	// make room for the caller
	for i in 0..set_size.checked_sub(1).expect("set size should be at least 1") {
		let validator_id = account("doogle", i, 0);
		authority_set.insert(validator_id);
	}
	authority_set.insert(caller);
	authority_set
}

// ======================================================================================
//            Helper methods to convert bytes to an associated type

fn aggkey_from_slice<T: Config<I>, I: 'static>(key: &[u8]) -> AggKeyFor<T, I> {
	let encoded = key.encode();
	AggKeyFor::<T, I>::decode(&mut &encoded[..]).unwrap()
}

// ======================================================================================

benchmarks_instance_pallet! {
	on_initialize_failure {
		let b in 1 .. 100;
		let current_block: T::BlockNumber = 0u32.into();
		KeygenResolutionPendingSince::<T, I>::put(current_block);
		let caller: T::AccountId = whitelisted_caller();
		let keygen_participants: BTreeSet<T::ValidatorId> = generate_authority_set::<T, I>(150, caller.clone().into());
		let blamed: BTreeSet<T::ValidatorId> = generate_authority_set::<T, I>(b, caller.clone().into());
		let mut keygen_response_status = KeygenResponseStatus::<T, I>::new(keygen_participants.clone());

		for validator_id in &keygen_participants {
			let _result = keygen_response_status.add_failure_vote(validator_id, blamed.clone());
		}

		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygen {
				keygen_ceremony_id: CEREMONY_ID,
				keygen_participants: keygen_participants.into_iter().collect(),
				response_status: keygen_response_status
			},
		);
	} : {
		Pallet::<T, I>::on_initialize(5u32.into());
	}
	verify {
		assert!(matches!(
			<Pallet::<T, I> as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Ready(Err(..))
		));
	}
	on_initialize_success {
		let current_block: T::BlockNumber = 0u32.into();
		KeygenResolutionPendingSince::<T, I>::put(current_block);
		let caller: T::AccountId = whitelisted_caller();
		let keygen_participants: BTreeSet<T::ValidatorId> = generate_authority_set::<T, I>(150, caller.clone().into());
		let mut keygen_response_status = KeygenResponseStatus::<T, I>::new(keygen_participants.clone());

		for validator_id in &keygen_participants {
			let _result = keygen_response_status.add_success_vote(
				validator_id,
				aggkey_from_slice::<T, I>(&NEW_PUBLIC_KEY[..])
			);
		}

		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygen {
				keygen_ceremony_id: CEREMONY_ID,
				keygen_participants: keygen_participants.into_iter().collect(),
				response_status: keygen_response_status
			},
		);
	} : {
		Pallet::<T, I>::on_initialize(5u32.into());
	}
	verify {
		assert_eq!(
			PendingVaultRotation::<T, I>::decode_variant(),
			Some(VaultRotationStatusVariant::AwaitingKeygenVerification),
		);
	}
	report_keygen_outcome {
		let caller: T::AccountId = whitelisted_caller();

		let keygen_participants = generate_authority_set::<T, I>(150, caller.clone().into());
		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygen {
				keygen_ceremony_id: CEREMONY_ID,
				keygen_participants: keygen_participants.clone().into_iter().collect(),
				response_status: KeygenResponseStatus::<T, I>::new(keygen_participants)
			},
		);
		use cf_chains::eth::sig_constants::{AGG_KEY_PUB, SIG};
		let bad_sig_byte = (SIG[SIG.len() - 1] + 1) % u8::MAX;
		let bad_sig = [SIG[..SIG.len() - 1].to_vec(), vec![bad_sig_byte]].concat();

		// Submit a key that doesn't verify the signature. This is approximately the same cost as success at time of writing.
		// But is much easier to write, and we might add slashing, which would increase the cost of the failure. Making this test the more
		// expensive of the two paths, therefore ensuring we have a more conservative benchmark
	} : _(RawOrigin::Signed(caller), CEREMONY_ID, ReportedKeygenOutcomeFor::<T, I>::Ok(aggkey_from_slice::<T, I>(&AGG_KEY_PUB)))
	verify {
		assert!(matches!(
			PendingVaultRotation::<T, I>::get().unwrap(),
			VaultRotationStatus::AwaitingKeygen { response_status, .. }
				if response_status.remaining_candidate_count() == 149
		))
	}
	vault_key_rotated {
		let new_public_key = aggkey_from_slice::<T, I>(&[0xbb; 33][..]);
		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingRotation { new_public_key },
		);
		let call = Call::<T, I>::vault_key_rotated {
			new_public_key: new_public_key,
			block_number: 5u64.into(),
			tx_hash: Decode::decode(&mut &TX_HASH[..]).unwrap()
		};
		let origin = T::EnsureWitnessedAtCurrentEpoch::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(Vaults::<T, I>::contains_key(T::EpochInfo::epoch_index()));
	}
	vault_key_rotated_externally {
		let origin = T::EnsureWitnessedAtCurrentEpoch::successful_origin();
		let new_public_key = aggkey_from_slice::<T, I>(&[0xbb; 33][..]);
		let call = Call::<T, I>::vault_key_rotated_externally {
			new_public_key: new_public_key,
			block_number: 5u64.into(),
			tx_hash: Decode::decode(&mut &TX_HASH[..]).unwrap()
		};
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(Vaults::<T, I>::contains_key(T::EpochInfo::epoch_index().saturating_add(1)));
	}
	set_keygen_timeout {
		let old_timeout: T::BlockNumber = 5u32.into();
		KeygenResponseTimeout::<T, I>::put(old_timeout);
		let new_timeout: T::BlockNumber = old_timeout + 1u32.into();
		// ensure it's a different value for most expensive path.
		let call = Call::<T, I>::set_keygen_timeout { new_timeout };
	} : { call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin())? }
	verify {
		assert_eq!(KeygenResponseTimeout::<T, I>::get(), new_timeout);
	}
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::MockRuntime,);
}
