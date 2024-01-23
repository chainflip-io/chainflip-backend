//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::ThresholdSigner;
use frame_benchmarking::{benchmarks_instance_pallet, whitelisted_caller};
use frame_support::traits::{EnsureOrigin, OnInitialize, UnfilteredDispatchable};
use frame_system::RawOrigin;

use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;

use cf_chains::benchmarking_value::BenchmarkValue;

fn insert_transaction_broadcast_attempt<T: pallet::Config<I>, I: 'static>(
	nominee: <T as Chainflip>::ValidatorId,
	broadcast_id: BroadcastId,
) {
	AwaitingBroadcast::<T, I>::insert(
		broadcast_id,
		BroadcastData::<T, I> {
			broadcast_id,
			transaction_payload: TransactionFor::<T, I>::benchmark_value(),
			threshold_signature_payload: PayloadFor::<T, I>::benchmark_value(),
			transaction_out_id: TransactionOutIdFor::<T, I>::benchmark_value(),
			nominee: Some(nominee),
		},
	);
	ThresholdSignatureData::<T, I>::insert(
		broadcast_id,
		(
			ApiCallFor::<T, I>::benchmark_value()
				.signed(&ThresholdSignatureFor::<T, I>::benchmark_value()),
			ThresholdSignatureFor::<T, I>::benchmark_value(),
		),
	);
	PendingBroadcasts::<T, I>::append(broadcast_id);
}

const INITIATED_AT: u32 = 100;

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
		broadcast_id: 1,
		initiated_at: INITIATED_AT.into(),
		should_broadcast: true,
	}
}

// TODO: check if we really reach the expensive parts of the code.
benchmarks_instance_pallet! {
	on_initialize {
		let caller: T::AccountId = whitelisted_caller();
		// We add one because one is added at genesis
		let timeout_block = frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get() + 1_u32.into();
		// Complexity parameter for expiry queue.

		let t in 1 .. 50u32;
		let r in 1 .. 50u32;
		let mut broadcast_id = 0;

		for _ in 1 .. t {
			broadcast_id += 1;
			insert_transaction_broadcast_attempt::<T, I>(caller.clone().into(), broadcast_id);
			Timeouts::<T, I>::mutate(timeout_block, |timeouts| timeouts.insert((broadcast_id, caller.clone().into())));
		}
		for _ in 1 .. r {
			broadcast_id += 1;
			insert_transaction_broadcast_attempt::<T, I>(caller.clone().into(), broadcast_id);
			DelayedBroadcastRetryQueue::<T, I>::append(timeout_block, broadcast_id);
		}
	} : {
		Pallet::<T, I>::on_initialize(timeout_block);
	}

	transaction_failed {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
		let broadcast_id = 15;
		insert_transaction_broadcast_attempt::<T, I>(caller.clone().into(), broadcast_id);
		frame_system::Pallet::<T>::set_block_number(10u32.into());
		let retry_block = frame_system::Pallet::<T>::block_number().saturating_add(
			T::RetryPolicy::next_attempt_delay(Pallet::<T, I>::attempt_count(broadcast_id))
				.unwrap_or(One::one()),
		);
	}: _(RawOrigin::Signed(caller), broadcast_id)
	verify {
		assert!(DelayedBroadcastRetryQueue::<T, I>::get(retry_block).contains(&broadcast_id));
	}

	on_signature_ready {
		let broadcast_id = 0;
		let timeout_block = frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get() + 1_u32.into();
		insert_transaction_broadcast_attempt::<T, I>(whitelisted_caller(), broadcast_id);
		let call = generate_on_signature_ready_call::<T, I>();
	} : { call.dispatch_bypass_filter(T::EnsureThresholdSigned::try_successful_origin().unwrap())? }
	verify {
		assert_eq!(BroadcastIdCounter::<T, I>::get(), 0);
		assert_eq!(Pallet::<T, I>::attempt_count(broadcast_id), 0);
		assert!(Timeouts::<T, I>::contains_key(timeout_block));
	}

	start_next_broadcast_attempt {
		let api_call = ApiCallFor::<T, I>::benchmark_value();
		let signed_api_call = api_call.signed(&BenchmarkValue::benchmark_value());
		let broadcast_id = <Pallet::<T, I> as Broadcaster<_>>::threshold_sign_and_broadcast(
			BenchmarkValue::benchmark_value(),
		);
		insert_transaction_broadcast_attempt::<T, I>(whitelisted_caller(), broadcast_id);
	} : {
		Pallet::<T, I>::start_next_broadcast_attempt(broadcast_id)
	}
	verify {
		assert!(AwaitingBroadcast::<T, I>::contains_key(broadcast_id));
	}

	transaction_succeeded {
		let caller: T::AccountId = whitelisted_caller();
		let signer_id = SignerIdFor::<T, I>::benchmark_value();
		let initiated_at: ChainBlockNumberFor<T, I> = INITIATED_AT.into();
		TransactionOutIdToBroadcastId::<T, I>::insert(TransactionOutIdFor::<T, I>::benchmark_value(), (1, initiated_at));

		let broadcast_id = 1;
		insert_transaction_broadcast_attempt::<T, I>(whitelisted_caller(), broadcast_id);
		TransactionMetadata::<T, I>::insert(
			broadcast_id,
			TransactionMetadataFor::<T, I>::benchmark_value(),
		);
		let call = Call::<T, I>::transaction_succeeded{
			tx_out_id: TransactionOutIdFor::<T, I>::benchmark_value(),
			signer_id,
			tx_fee: TransactionFeeFor::<T, I>::benchmark_value(),
			tx_metadata: TransactionMetadataFor::<T, I>::benchmark_value(),
		};
	} : { call.dispatch_bypass_filter(T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap())? }
	verify {
		// We expect the unwrap to error if the extrinsic didn't fire an event - if an event has been emitted we reached the end of the extrinsic
		let _ = frame_system::Pallet::<T>::events().pop().expect("No event has been emitted from the transaction_succeeded extrinsic").event;
	}
}
