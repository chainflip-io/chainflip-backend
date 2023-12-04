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
				broadcast_attempt_id,
				transaction_payload: TransactionFor::<T, I>::benchmark_value(),
				threshold_signature_payload: PayloadFor::<T, I>::benchmark_value(),
				transaction_out_id: TransactionOutIdFor::<T, I>::benchmark_value(),
			},
			nominee,
		},
	);
}

const INITIATED_AT: u32 = 100;

pub type AggKeyFor<T, I> = <<<T as pallet::Config<I>>::TargetChain as cf_chains::Chain>::ChainCrypto as ChainCrypto>::AggKey;

// Generates a new signature ready call.
fn generate_on_signature_ready_call<T: pallet::Config<I>, I>() -> pallet::Call<T, I> {
	let threshold_request_id = 1;
	T::ThresholdSigner::insert_signature(
		threshold_request_id,
		ThresholdSignatureFor::<T, I>::benchmark_value(),
	);
	Call::<T, I>::on_signature_ready {
		threshold_request_id,
		threshold_signature_payload: PayloadFor::<T, I>::benchmark_value(),
		api_call: Box::new(ApiCallFor::<T, I>::benchmark_value()),
		broadcast_attempt_id: BroadcastAttemptId { broadcast_id: 1, attempt_count: 0 },
		initiated_at: INITIATED_AT.into(),
		should_broadcast: true,
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
		let valid_key = AggKeyFor::<T, I>::benchmark_value();
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
		generate_on_signature_ready_call::<T, I>().dispatch_bypass_filter(T::EnsureThresholdSigned::try_successful_origin().unwrap())?;
		let expiry_block = frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get();
		let valid_key = AggKeyFor::<T, I>::benchmark_value();
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
		let valid_key = AggKeyFor::<T, I>::benchmark_value();
	} : { call.dispatch_bypass_filter(T::EnsureThresholdSigned::try_successful_origin().unwrap())? }
	verify {
		assert_eq!(BroadcastIdCounter::<T, I>::get(), 0);
		assert_eq!(BroadcastAttemptCount::<T, I>::get(broadcast_id), 0);
		assert!(Timeouts::<T, I>::contains_key(timeout_block));
	}
	start_next_broadcast_attempt {
		let api_call = ApiCallFor::<T, I>::benchmark_value();
		let signed_api_call = api_call.signed(&BenchmarkValue::benchmark_value());
		let broadcast_id = <Pallet::<T, I> as Broadcaster<_>>::threshold_sign_and_broadcast(
			BenchmarkValue::benchmark_value(),
		);
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id,
			attempt_count: BroadcastAttemptCount::<T, I>::get(broadcast_id),
		};

		let transaction_payload = TransactionFor::<T, I>::benchmark_value();

	} : {
		Pallet::<T, I>::start_next_broadcast_attempt( BroadcastAttempt::<T, I> {
			broadcast_attempt_id,
			transaction_payload,
			threshold_signature_payload: PayloadFor::<T, I>::benchmark_value(),
			transaction_out_id: TransactionOutIdFor::<T, I>::benchmark_value(),
		})
	}
	verify {
		assert!(AwaitingBroadcast::<T, I>::contains_key(broadcast_attempt_id.peek_next()));
	}
	transaction_succeeded {
		let caller: T::AccountId = whitelisted_caller();
		let signer_id = SignerIdFor::<T, I>::benchmark_value();
		let initiated_at: ChainBlockNumberFor<T, I> = INITIATED_AT.into();
		TransactionOutIdToBroadcastId::<T, I>::insert(TransactionOutIdFor::<T, I>::benchmark_value(), (1, initiated_at));

		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 0
		};
		insert_transaction_broadcast_attempt::<T, I>(whitelisted_caller(), broadcast_attempt_id);
		let call = Call::<T, I>::transaction_succeeded{
			tx_out_id: TransactionOutIdFor::<T, I>::benchmark_value(),
			signer_id,
			tx_fee: TransactionFeeFor::<T, I>::benchmark_value(),
			tx_metadata: TransactionMetadataFor::<T, I>::benchmark_value(),
		};
		let valid_key = AggKeyFor::<T, I>::benchmark_value();
	} : { call.dispatch_bypass_filter(T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap())? }
	verify {
		// We expect the unwrap to error if the extrinsic didn't fire an event - if an event has been emitted we reached the end of the extrinsic
		let _ = frame_system::Pallet::<T>::events().pop().expect("No event has been emitted from the transaction_succeeded extrinsic").event;
	}
}
