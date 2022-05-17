//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::eth::{TransactionHash, H256};
use cf_traits::ThresholdSigner;
use frame_benchmarking::{benchmarks_instance_pallet, whitelisted_caller};
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};
use frame_system::RawOrigin;

use cf_chains::benchmarking_default::BenchmarkDefault;

type SignerIdFor<T, I> = <<T as Config<I>>::TargetChain as ChainAbi>::SignerCredential;
type SignedTransactionFor<T, I> = <<T as Config<I>>::TargetChain as ChainAbi>::SignedTransaction;
type ApiCallFor<T, I> = <T as Config<I>>::ApiCall;
type ThresholdSignatureFor<T, I> =
	<<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;
type ChainAmountFor<T, I> = <<T as Config<I>>::TargetChain as cf_chains::Chain>::ChainAmount;
type TransactionHashFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::TransactionHash;

benchmarks_instance_pallet! {
	on_initialize {} : {}
	transaction_ready_for_transmission {
		let caller: T::AccountId = whitelisted_caller();
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 1
		};
		let signed_tx = SignedTransactionFor::<T, I>::benchmark_default();
		let signer_id = SignerIdFor::<T, I>::benchmark_default();
	} : _(RawOrigin::Signed(caller), broadcast_attempt_id, signed_tx, signer_id)
	transmission_failure {
		let origin = T::EnsureWitnessed::successful_origin();
		let transaction_hash = TransactionHashFor::<T, I>::benchmark_default();
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 1
		};
		let tf = TransmissionFailure::TransactionRejected;
		let call = Call::<T, I>::transmission_failure { broadcast_attempt_id: broadcast_attempt_id, failure: tf, tx_hash: transaction_hash };
	} : { call.dispatch_bypass_filter(origin)? }
	on_signature_ready {
		let origin = T::EnsureThresholdSigned::successful_origin();
		let threshold_request_id = <T::ThresholdSigner as ThresholdSigner<T::TargetChain>>::RequestId::benchmark_default();
		let api_call = ApiCallFor::<T, I>::benchmark_default();
		let call = Call::<T, I>::on_signature_ready{threshold_request_id, api_call};
	} : { call.dispatch_bypass_filter(origin)? }
	signature_accepted {
		let origin = T::EnsureThresholdSigned::successful_origin();
		let payload = ThresholdSignatureFor::<T, I>::benchmark_default();
		let tx_signer = SignerIdFor::<T, I>::benchmark_default();
		let tx_fee = ChainAmountFor::<T, I>::benchmark_default();
		let block_number = 1;
		let tx_hash = TransactionHashFor::<T, I>::benchmark_default();
		let call = Call::<T, I>::signature_accepted{payload, tx_signer, tx_fee, block_number, tx_hash};
	} : { call.dispatch_bypass_filter(origin)? }
}
