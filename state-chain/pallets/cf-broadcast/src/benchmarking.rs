// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

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
	PendingApiCalls::<T, I>::insert(
		broadcast_id,
		ApiCallFor::<T, I>::benchmark_value().signed(
			&ThresholdSignatureFor::<T, I>::benchmark_value(),
			AggKey::<T, I>::benchmark_value(),
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
		AggKey::<T, I>::benchmark_value(),
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
	fn update_pallet_config() {
		let blocks: u32 = 30;
		let call = Call::<T, I>::update_pallet_config {
			update: PalletConfigUpdate::BroadcastTimeout { blocks },
		};
		let o = T::EnsureGovernance::try_successful_origin().unwrap();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(o));
		}

		assert_eq!(BroadcastTimeout::<T, I>::get(), blocks.into())
	}

	#[benchmark]
	fn on_initialize(t: Linear<1, 50>, r: Linear<1, 50>) {
		let caller: T::AccountId = whitelisted_caller();
		// We add one because one is added at genesis
		let timeout_target_block = T::ChainTracking::get_block_height() +
			crate::BroadcastTimeout::<T, I>::get() +
			1u32.into();
		let timeout_block = frame_system::Pallet::<T>::block_number() + 1_u32.into();
		// Complexity parameter for expiry queue.

		let mut broadcast_id = 0;

		for _ in 1..t {
			broadcast_id += 1;
			insert_transaction_broadcast_attempt::<T, I>(Some(caller.clone().into()), broadcast_id);
			Timeouts::<T, I>::append((
				timeout_target_block,
				broadcast_id,
				T::ValidatorId::from(caller.clone()),
			));
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
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
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
		let timeout_chainblock =
			T::ChainTracking::get_block_height() + crate::BroadcastTimeout::<T, I>::get();
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
		assert!(Timeouts::<T, I>::get()
			.iter()
			.any(|(chainblock, _, _)| *chainblock == timeout_chainblock));
	}

	#[benchmark]
	fn start_next_broadcast_attempt() {
		let (broadcast_id, _) = Pallet::<T, I>::threshold_sign_and_broadcast(
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
			transaction_ref: TransactionRefFor::<T, I>::benchmark_value(),
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
