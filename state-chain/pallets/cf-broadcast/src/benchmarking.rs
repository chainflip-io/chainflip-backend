//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::ThresholdSigner;
use frame_benchmarking::{benchmarks_instance_pallet, whitelisted_caller};
use frame_support::{
	dispatch::UnfilteredDispatchable,
	traits::{EnsureOrigin, OnInitialize},
};
use frame_system::RawOrigin;

use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;

use cf_chains::benchmarking_value::BenchmarkValue;

fn insert_transaction_broadcast_attempt<T: pallet::Config<I>, I: 'static>(
	nominee: <T as Chainflip>::ValidatorId,
	broadcast_attempt_id: BroadcastAttemptId,
) {
	AwaitingBroadcast::<T, I>::insert(
		broadcast_attempt_id,
		TransactionSigningAttempt {
			broadcast_attempt: BroadcastAttempt::<T, I> {
				transaction_payload: TransactionFor::<T, I>::benchmark_value(),
				broadcast_attempt_id,
			},
			nominee,
		},
	);
}

// Generates a new signature ready call.
fn generate_on_signature_ready_call<T: pallet::Config<I>, I>() -> pallet::Call<T, I> {
	let threshold_request_id =
		<T::ThresholdSigner as ThresholdSigner<T::TargetChain>>::RequestId::benchmark_value();
	T::ThresholdSigner::insert_signature(
		threshold_request_id,
		ThresholdSignatureFor::<T, I>::benchmark_value(),
	);
	Call::<T, I>::on_signature_ready {
		threshold_request_id,
		api_call: Box::new(ApiCallFor::<T, I>::benchmark_value()),
		broadcast_id: 1,
	}
}

// TODO: check if we really reach the expensive parts of the code.
benchmarks_instance_pallet! {
	on_initialize {
		// We add one because one is added at genesis
		let timeout_block = frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get() + 1_u32.into();
		// Complexity parameter for expiry queue.
		let x in 1 .. 1000u32;
		for i in 1 .. x {
			let broadcast_attempt_id = BroadcastAttemptId {broadcast_id: i, attempt_count: 1};
			Timeouts::<T, I>::append(timeout_block, broadcast_attempt_id);
			ThresholdSignatureData::<T, I>::insert(i, (ApiCallFor::<T, I>::benchmark_value(), ThresholdSignatureFor::<T, I>::benchmark_value()))
		}
		let valid_key = <<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value();
		T::KeyProvider::set_key(valid_key);
	} : {
		Pallet::<T, I>::on_initialize(timeout_block);
	}
	// TODO: add a benchmark for the failure case
	transaction_signing_failure {
		// TODO: This benchmark is the success case. The failure case is not yet implemented and can be quite expensive in the worst case.
		// Unfortunately with the current implementation, there is no good way to determine this before we execute the benchmark.
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 1
		};
		insert_transaction_broadcast_attempt::<T, I>(caller.clone().into(), broadcast_attempt_id);
		generate_on_signature_ready_call::<T, I>().dispatch_bypass_filter(T::EnsureThresholdSigned::successful_origin())?;
		let expiry_block = frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get();
		let valid_key = <<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value();
		T::KeyProvider::set_key(valid_key);
	}: _(RawOrigin::Signed(caller), broadcast_attempt_id)
	verify {
		assert!(Timeouts::<T, I>::contains_key(expiry_block));
	}
	on_signature_ready {
		let broadcast_id = 0;
		let timeout_block = frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get() + 1_u32.into();
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id,
			attempt_count: 0
		};
		insert_transaction_broadcast_attempt::<T, I>(whitelisted_caller(), broadcast_attempt_id);
		let call = generate_on_signature_ready_call::<T, I>();
		let valid_key = <<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value();
		T::KeyProvider::set_key(valid_key);
	} : { call.dispatch_bypass_filter(T::EnsureThresholdSigned::successful_origin())? }
	verify {
		assert_eq!(BroadcastIdCounter::<T, I>::get(), 0);
		assert_eq!(BroadcastAttemptCount::<T, I>::get(broadcast_id), 0);
		assert!(Timeouts::<T, I>::contains_key(timeout_block));
	}
	start_next_broadcast_attempt {
		let broadcast_attempt_id = Pallet::<T, I>::start_broadcast(&BenchmarkValue::benchmark_value(), BenchmarkValue::benchmark_value(), BenchmarkValue::benchmark_value(), 1);

		T::KeyProvider::set_key(<<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value());
		let transaction_payload = TransactionFor::<T, I>::benchmark_value();

	} : {
		Pallet::<T, I>::start_next_broadcast_attempt( BroadcastAttempt::<T, I> {
			broadcast_attempt_id,
			transaction_payload,
		})
	}
	verify {
		assert!(AwaitingBroadcast::<T, I>::contains_key(broadcast_attempt_id.next_attempt()));
	}
	signature_accepted {
		let caller: T::AccountId = whitelisted_caller();
		let signer_id = SignerIdFor::<T, I>::benchmark_value();
		SignatureToBroadcastIdLookup::<T, I>::insert(ThresholdSignatureFor::<T, I>::benchmark_value(), 1);

		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 0
		};
		insert_transaction_broadcast_attempt::<T, I>(whitelisted_caller(), broadcast_attempt_id);
		let call = Call::<T, I>::signature_accepted{
			signature: ThresholdSignatureFor::<T, I>::benchmark_value(),
			signer_id,
			tx_fee: TransactionFeeFor::<T, I>::benchmark_value(),
		};
		let valid_key = <<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value();
		T::KeyProvider::set_key(valid_key);
	} : { call.dispatch_bypass_filter(T::EnsureWitnessedAtCurrentEpoch::successful_origin())? }
	verify {
		// We expect the unwrap to error if the extrinsic didn't fire an event - if an event has been emitted we reached the end of the extrinsic
		let _ = frame_system::Pallet::<T>::events().pop().expect("No event has been emitted from the signature_accepted extrinsic").event;
	}
}
