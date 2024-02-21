#![cfg(feature = "runtime-benchmarks")]
use super::*;

use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, ThresholdSigner};
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{EnsureOrigin, Hooks, UnfilteredDispatchable},
};
use frame_system::RawOrigin;

fn insert_transaction_broadcast_attempt<T: pallet::Config<I>, I: 'static>(
	nominee: Option<<T as Chainflip>::ValidatorId>,
	broadcast_id: BroadcastId,
) {
	AwaitingBroadcast::<T, I>::insert(
		broadcast_id,
		BroadcastData::<T, I> {
			broadcast_id,
			transaction_payload: TransactionFor::<T, I>::benchmark_value(),
			threshold_signature_payload: PayloadFor::<T, I>::benchmark_value(),
			transaction_out_id: TransactionOutIdFor::<T, I>::benchmark_value(),
			nominee,
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

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_initialize(t: Linear<1, 50>, r: Linear<1, 50>) {
		let caller: T::AccountId = whitelisted_caller();
		// We add one because one is added at genesis
		let timeout_block =
			frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get() + 1_u32.into();
		// Complexity parameter for expiry queue.

		let mut broadcast_id = 0;

		for _ in 1..t {
			broadcast_id += 1;
			insert_transaction_broadcast_attempt::<T, I>(Some(caller.clone().into()), broadcast_id);
			Timeouts::<T, I>::mutate(timeout_block, |timeouts| {
				timeouts.insert((broadcast_id, caller.clone().into()))
			});
		}
		for _ in 1..r {
			broadcast_id += 1;
			insert_transaction_broadcast_attempt::<T, I>(Some(caller.clone().into()), broadcast_id);
			DelayedBroadcastRetryQueue::<T, I>::append(timeout_block, broadcast_id);
		}

		#[block]
		{
			Pallet::<T, I>::on_initialize(timeout_block);
		}
	}

	#[benchmark]
	fn transaction_failed() {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(&caller, AccountRole::Validator);
		let broadcast_id = 15;
		insert_transaction_broadcast_attempt::<T, I>(Some(caller.clone().into()), broadcast_id);
		frame_system::Pallet::<T>::set_block_number(10u32.into());
		let retry_block = frame_system::Pallet::<T>::block_number().saturating_add(
			T::RetryPolicy::next_attempt_delay(Pallet::<T, I>::attempt_count(broadcast_id) + 1)
				.unwrap_or(One::one()),
		);

		#[extrinsic_call]
		transaction_failed(RawOrigin::Signed(caller), broadcast_id);

		assert!(DelayedBroadcastRetryQueue::<T, I>::get(retry_block).contains(&broadcast_id));
	}

	#[benchmark]
	fn on_signature_ready() {
		let broadcast_id = 0;
		frame_system::Pallet::<T>::set_block_number(100u32.into());
		let timeout_block = frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get();
		insert_transaction_broadcast_attempt::<T, I>(Some(whitelisted_caller()), broadcast_id);
		let call = generate_on_signature_ready_call::<T, I>();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(
				T::EnsureThresholdSigned::try_successful_origin().unwrap()
			));
		}

		assert_eq!(BroadcastIdCounter::<T, I>::get(), 0);
		assert_eq!(Pallet::<T, I>::attempt_count(broadcast_id), 0);
		assert!(Timeouts::<T, I>::contains_key(timeout_block));
	}

	#[benchmark]
	fn start_next_broadcast_attempt() {
		let broadcast_id = Pallet::<T, I>::threshold_sign_and_broadcast(
			BenchmarkValue::benchmark_value(),
			None,
			|_| None,
		);
		insert_transaction_broadcast_attempt::<T, I>(None, broadcast_id);

		#[block]
		{
			Pallet::<T, I>::start_next_broadcast_attempt(broadcast_id)
		}

		assert!(AwaitingBroadcast::<T, I>::get(broadcast_id).unwrap().nominee.is_some());
	}

	#[benchmark]
	fn transaction_succeeded() {
		let signer_id = SignerIdFor::<T, I>::benchmark_value();
		let initiated_at: ChainBlockNumberFor<T, I> = INITIATED_AT.into();
		TransactionOutIdToBroadcastId::<T, I>::insert(
			TransactionOutIdFor::<T, I>::benchmark_value(),
			(1, initiated_at),
		);

		let broadcast_id = 1;
		insert_transaction_broadcast_attempt::<T, I>(Some(whitelisted_caller()), broadcast_id);
		TransactionMetadata::<T, I>::insert(
			broadcast_id,
			TransactionMetadataFor::<T, I>::benchmark_value(),
		);
		let call = Call::<T, I>::transaction_succeeded {
			tx_out_id: TransactionOutIdFor::<T, I>::benchmark_value(),
			signer_id,
			tx_fee: TransactionFeeFor::<T, I>::benchmark_value(),
			tx_metadata: TransactionMetadataFor::<T, I>::benchmark_value(),
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(
				T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap(),
			));
		}

		// Storage is cleaned up upon successful broadcast
		assert!(TransactionOutIdToBroadcastId::<T, I>::get(
			TransactionOutIdFor::<T, I>::benchmark_value()
		)
		.is_none());
		assert!(TransactionMetadata::<T, I>::get(broadcast_id).is_none());
	}

	#[cfg(test)]
	use crate::mock::*;

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_on_initialize::<Test, Instance1>(50, 50, true);
		});
		new_test_ext().execute_with(|| {
			_transaction_failed::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_on_signature_ready::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_start_next_broadcast_attempt::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_transaction_succeeded::<Test, Instance1>(true);
		});
	}
}
