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

use cf_chains::benchmarking_value::BenchmarkValue;

// Inserts a new transaction signing attempt into the storage.
fn insert_transaction_signing_attempt<T: pallet::Config<I>, I: 'static>(
	nominee: <T as Chainflip>::ValidatorId,
	broadcast_attempt_id: BroadcastAttemptId,
) {
	AwaitingTransactionSignature::<T, I>::insert(
		broadcast_attempt_id,
		TransactionSigningAttempt {
			broadcast_attempt: BroadcastAttempt::<T, I> {
				unsigned_tx: UnsignedTransactionFor::<T, I>::benchmark_value(),
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
		api_call: ApiCallFor::<T, I>::benchmark_value(),
	}
}

// TODO: check if we really reach the expensive parts of the code.
benchmarks_instance_pallet! {
	on_initialize {
		let expiry_block = T::BlockNumber::from(6u32);
		// Complexity parameter for expiry queue.
		let x in 1 .. 1000u32;
		for i in 1 .. x {
			let broadcast_attempt_id = BroadcastAttemptId {broadcast_id: i, attempt_count: 1};
			Expiries::<T, I>::mutate(expiry_block, |entries| {
				entries.push((BroadcastStage::TransactionSigning, broadcast_attempt_id))
			});
			ThresholdSignatureData::<T, I>::insert(i, (ApiCallFor::<T, I>::benchmark_value(), ThresholdSignatureFor::<T, I>::benchmark_value()))
		}
		let valid_key = <<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value();
		T::KeyProvider::set_key(valid_key);
	} : {
		Pallet::<T, I>::on_initialize(expiry_block);
	}
	transaction_ready_for_transmission {
		let caller: T::AccountId = whitelisted_caller();
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 1
		};
		insert_transaction_signing_attempt::<T, I>(caller.clone().into(), broadcast_attempt_id);
		generate_on_signature_ready_call::<T, I>().dispatch_bypass_filter(T::EnsureThresholdSigned::successful_origin())?;
		let valid_key = <<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value();
		T::KeyProvider::set_key(valid_key);
	} : _(RawOrigin::Signed(caller), broadcast_attempt_id, SignedTransactionFor::<T, I>::benchmark_value(), SignerIdFor::<T, I>::benchmark_value())
	verify {
		assert!(Expiries::<T, I>::contains_key(frame_system::Pallet::<T>::block_number() + T::TransmissionTimeout::get()));
	}
	// TODO: add a benchmark for the failure case
	transaction_signing_failure {
		// TODO: This benchmark is the success case. The failure case is not yet implemented and can be quite expensive in the worst case.
		// Unfortunately with the current implementation, there is no good way to determine this before we execute the benchmark.
		let caller: T::AccountId = whitelisted_caller();
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 1
		};
		insert_transaction_signing_attempt::<T, I>(caller.clone().into(), broadcast_attempt_id);
		generate_on_signature_ready_call::<T, I>().dispatch_bypass_filter(T::EnsureThresholdSigned::successful_origin())?;
		let expiry_block = frame_system::Pallet::<T>::block_number() + T::SigningTimeout::get();
		let valid_key = <<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value();
		T::KeyProvider::set_key(valid_key);
	}: _(RawOrigin::Signed(caller), broadcast_attempt_id)
	verify {
		assert!(Expiries::<T, I>::contains_key(expiry_block));
	}
	on_signature_ready {
		let should_expire_in = T::BlockNumber::from(6u32);
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 1
		};
		insert_transaction_signing_attempt::<T, I>(whitelisted_caller(), broadcast_attempt_id);
		let call = generate_on_signature_ready_call::<T, I>();
		let valid_key = <<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value();
		T::KeyProvider::set_key(valid_key);
	} : { call.dispatch_bypass_filter(T::EnsureThresholdSigned::successful_origin())? }
	verify {
		assert_eq!(BroadcastIdCounter::<T, I>::get(), 1);
		assert!(BroadcastIdToAttemptNumbers::<T, I>::contains_key(1));
		assert!(Expiries::<T, I>::contains_key(should_expire_in));
	}
	start_next_broadcast_attempt {
		let broadcast_attempt_id = Pallet::<T, I>::start_broadcast(&BenchmarkValue::benchmark_value(), BenchmarkValue::benchmark_value(), BenchmarkValue::benchmark_value());

		T::KeyProvider::set_key(<<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value());
		let unsigned_tx = UnsignedTransactionFor::<T, I>::benchmark_value();

	} : {
		Pallet::<T, I>::start_next_broadcast_attempt( BroadcastAttempt::<T, I> {
			broadcast_attempt_id,
			unsigned_tx,
		})
	}
	verify {
		assert!(AwaitingTransactionSignature::<T, I>::contains_key(broadcast_attempt_id.next_attempt()));
	}
	signature_accepted {
		let caller: T::AccountId = whitelisted_caller();
		SignerIdToAccountId::<T, I>::insert(SignerIdFor::<T, I>::benchmark_value(), caller);
		SignatureToBroadcastIdLookup::<T, I>::insert(ThresholdSignatureFor::<T, I>::benchmark_value(), 1);
		let call = Call::<T, I>::signature_accepted{
			signature: ThresholdSignatureFor::<T, I>::benchmark_value(),
			tx_signer: SignerIdFor::<T, I>::benchmark_value(),
			tx_fee: ChainAmountFor::<T, I>::default(),
			block_number: 1,
			tx_hash: TransactionHashFor::<T, I>::default()
		};
		let valid_key = <<T as Config<I>>::TargetChain as ChainCrypto>::AggKey::benchmark_value();
		T::KeyProvider::set_key(valid_key);
	} : { call.dispatch_bypass_filter(T::EnsureWitnessedAtCurrentEpoch::successful_origin())? }
	verify {
		// We expect the unwrap to error if the extrinsic didn't fire an event - if an event has been emitted we reached the end of the extrinsic
		let _ = frame_system::Pallet::<T>::events().pop().expect("No event has been emitted from the signature_accepted extrinsic").event;
	}
}
