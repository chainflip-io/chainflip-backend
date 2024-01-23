//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::Pallet;
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::GENESIS_EPOCH;
use cf_traits::{AccountRoleRegistry, EpochInfo};
use codec::Decode;
use frame_benchmarking::{account, benchmarks_instance_pallet, whitelisted_caller};
use frame_support::traits::{OnNewAccount, UnfilteredDispatchable};
use frame_system::RawOrigin;

// Note: Currently we only have one chain (ETH) - as soon we've
// another chain we've to take this in account in our weight calculation benchmark.

const CEREMONY_ID: u64 = 1;
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

benchmarks_instance_pallet! {
	on_initialize_failure {
		let b in 1 .. 100;
		let current_block: BlockNumberFor<T> = 0u32.into();
		KeygenResolutionPendingSince::<T, I>::put(current_block);
		let caller: T::AccountId = whitelisted_caller();
		let keygen_participants: BTreeSet<T::ValidatorId> = generate_authority_set::<T, I>(150, caller.clone().into());
		let blamed: BTreeSet<T::ValidatorId> = generate_authority_set::<T, I>(b, caller.into());
		let mut keygen_response_status = KeygenResponseStatus::<T, I>::new(keygen_participants.clone());

		for validator_id in &keygen_participants {
			keygen_response_status.add_failure_vote(validator_id, blamed.clone());
		}

		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygen {
				ceremony_id: CEREMONY_ID,
				keygen_participants: keygen_participants.into_iter().collect(),
				response_status: keygen_response_status,
				new_epoch_index: GENESIS_EPOCH,
			},
		);
	} : {
		Pallet::<T, I>::on_initialize(5u32.into());
	}
	verify {
		assert!(matches!(
			<Pallet::<T, I> as VaultRotator>::status(),
			AsyncResult::Ready(VaultStatus::Failed(..))
		));
	}
	on_initialize_success {
		let current_block: BlockNumberFor<T> = 0u32.into();
		KeygenResolutionPendingSince::<T, I>::put(current_block);
		let caller: T::AccountId = whitelisted_caller();
		let keygen_participants: BTreeSet<T::ValidatorId> = generate_authority_set::<T, I>(150, caller.into());
		let mut keygen_response_status = KeygenResponseStatus::<T, I>::new(keygen_participants.clone());

		for validator_id in &keygen_participants {
			keygen_response_status.add_success_vote(
				validator_id,
				AggKeyFor::<T, I>::benchmark_value()
			);
		}

		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygen {
				ceremony_id: CEREMONY_ID,
				keygen_participants: keygen_participants.into_iter().collect(),
				response_status: keygen_response_status,
				new_epoch_index: GENESIS_EPOCH,
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
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		T::AccountRoleRegistry::register_as_validator(&caller).unwrap();

		let keygen_participants = generate_authority_set::<T, I>(150, caller.clone().into());
		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygen {
				ceremony_id: CEREMONY_ID,
				keygen_participants: keygen_participants.clone().into_iter().collect(),
				response_status: KeygenResponseStatus::<T, I>::new(keygen_participants),
				new_epoch_index: GENESIS_EPOCH,
			},
		);
		use cf_chains::eth::sig_constants::SIG;
		let bad_sig_byte = (SIG[SIG.len() - 1] + 1) % u8::MAX;
		let bad_sig = [SIG[..SIG.len() - 1].to_vec(), vec![bad_sig_byte]].concat();

		// Submit a key that doesn't verify the signature. This is approximately the same cost as success at time of writing.
		// But is much easier to write, and we might add slashing, which would increase the cost of the failure. Making this test the more
		// expensive of the two paths, therefore ensuring we have a more conservative benchmark
	} : _(RawOrigin::Signed(caller), CEREMONY_ID, KeygenOutcomeFor::<T, I>::Ok(AggKeyFor::<T, I>::benchmark_value()))
	verify {
		assert!(matches!(
			PendingVaultRotation::<T, I>::get().unwrap(),
			VaultRotationStatus::AwaitingKeygen { response_status, .. }
				if response_status.remaining_candidate_count() == 149
		))
	}
	on_keygen_verification_result {
		let caller: T::AccountId = whitelisted_caller();
		let agg_key = AggKeyFor::<T, I>::benchmark_value();
		let keygen_participants = generate_authority_set::<T, I>(150, caller.into());
		let request_id = Pallet::<T, I>::trigger_keygen_verification(CEREMONY_ID, agg_key, keygen_participants.into_iter().collect(), 2);
		T::ThresholdSigner::insert_signature(
			request_id,
			ThresholdSignatureFor::<T, I>::benchmark_value(),
		);
		let call = Call::<T, I>::on_keygen_verification_result {
			keygen_ceremony_id: CEREMONY_ID,
			threshold_request_id: request_id,
			new_public_key: agg_key,
		};
		let origin = T::EnsureThresholdSigned::try_successful_origin().unwrap();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(matches!(
			PendingVaultRotation::<T, I>::get().unwrap(),
			VaultRotationStatus::KeygenVerificationComplete { new_public_key }
				if new_public_key == agg_key
		))
	}
	vault_key_rotated_externally {
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		let call = Call::<T, I>::vault_key_rotated_externally {
			new_public_key: AggKeyFor::<T, I>::benchmark_value(),
			block_number: 5u32.into(),
			tx_id: Decode::decode(&mut &TX_HASH[..]).unwrap()
		};
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(Vaults::<T, I>::contains_key(T::EpochInfo::epoch_index().saturating_add(1)));
	}
	set_keygen_response_timeout {
		let old_timeout: BlockNumberFor<T> = 5u32.into();
		KeygenResponseTimeout::<T, I>::put(old_timeout);
		let new_timeout: BlockNumberFor<T> = old_timeout + 1u32.into();
		// ensure it's a different value for most expensive path.
		let call = Call::<T, I>::set_keygen_response_timeout { new_timeout };
	} : { call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())? }
	verify {
		assert_eq!(KeygenResponseTimeout::<T, I>::get(), new_timeout);
	}
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
