//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::ThresholdSigner;
use frame_benchmarking::{benchmarks_instance_pallet, whitelisted_caller};
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};
use frame_system::RawOrigin;

use cf_chains::benchmarking_default::BenchmarkDefault;
use frame_benchmarking::impl_benchmark_test_suite;

type SignerIdFor<T, I> = <<T as Config<I>>::TargetChain as ChainAbi>::SignerCredential;
type SignedTransactionFor<T, I> = <<T as Config<I>>::TargetChain as ChainAbi>::SignedTransaction;
type ApiCallFor<T, I> = <T as Config<I>>::ApiCall;
type ThresholdSignatureFor<T, I> =
	<<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;
type ChainAmountFor<T, I> = <<T as Config<I>>::TargetChain as cf_chains::Chain>::ChainAmount;
type TransactionHashFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::TransactionHash;

fn setup_signature<T: pallet::Config<I>, I>() -> pallet::Call<T, I> {
	let threshold_request_id =
		<T::ThresholdSigner as ThresholdSigner<T::TargetChain>>::RequestId::benchmark_default();
	let api_call = ApiCallFor::<T, I>::benchmark_default();
	let call = Call::<T, I>::on_signature_ready { threshold_request_id, api_call };
	return call
}

benchmarks_instance_pallet! {
	on_initialize {} : {}
	transaction_ready_for_transmission {
		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureThresholdSigned::successful_origin();
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 1
		};
		let signed_tx = SignedTransactionFor::<T, I>::benchmark_default();
		let signer_id = SignerIdFor::<T, I>::benchmark_default();
		let sd = setup_signature::<T, I>();
		sd.dispatch_bypass_filter(origin)?;
	} : _(RawOrigin::Signed(caller), broadcast_attempt_id, signed_tx, signer_id)
	transaction_signing_failure {
		let caller: T::AccountId = whitelisted_caller();
		let api_call = ApiCallFor::<T, I>::benchmark_default();
		let threshold_signature = ThresholdSignatureFor::<T, I>::benchmark_default();
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 1
		};
		let origin = T::EnsureThresholdSigned::successful_origin();
		let expiry_block = frame_system::Pallet::<T>::block_number() + T::SigningTimeout::get();
		let threshold_request_id = <T::ThresholdSigner as ThresholdSigner<T::TargetChain>>::RequestId::benchmark_default();
		let api_call = ApiCallFor::<T, I>::benchmark_default();
		let call = Call::<T, I>::on_signature_ready{threshold_request_id, api_call};
		call.dispatch_bypass_filter(origin)?;
	}: _(RawOrigin::Signed(caller), broadcast_attempt_id)
	verify {
		assert!(Expiries::<T, I>::contains_key(expiry_block));
	}
	on_signature_ready {
		let origin = T::EnsureThresholdSigned::successful_origin();
		let expiry_block = frame_system::Pallet::<T>::block_number() + T::SigningTimeout::get();
		let threshold_request_id = <T::ThresholdSigner as ThresholdSigner<T::TargetChain>>::RequestId::benchmark_default();
		let api_call = ApiCallFor::<T, I>::benchmark_default();
		let call = Call::<T, I>::on_signature_ready{threshold_request_id, api_call};
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(BroadcastIdCounter::<T, I>::get(), 1);
		assert!(BroadcastIdToAttemptNumbers::<T, I>::contains_key(1));
		assert!(Expiries::<T, I>::contains_key(expiry_block));
	}
	signature_accepted {
		let origin = T::EnsureThresholdSigned::successful_origin();
		let payload = ThresholdSignatureFor::<T, I>::benchmark_default();
		let tx_signer = SignerIdFor::<T, I>::benchmark_default();
		let tx_fee = ChainAmountFor::<T, I>::default();
		let block_number = 1;
		let tx_hash = TransactionHashFor::<T, I>::benchmark_default();
		let call = Call::<T, I>::signature_accepted{payload, tx_signer, tx_fee, block_number, tx_hash};
	} : { call.dispatch_bypass_filter(origin)? }
}
