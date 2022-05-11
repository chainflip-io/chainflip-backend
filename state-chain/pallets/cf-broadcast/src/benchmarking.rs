//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::eth::{TransactionHash, H256};
use cf_runtime_benchmark_utilities::BenchmarkDefault;
use frame_benchmarking::{benchmarks_instance_pallet, whitelisted_caller};
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};
use frame_system::RawOrigin;

// use cf_runtime_benchmark_utilities::BenchmarkDefault;

// type TransactionHashFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::TransactionHash;
type SignerIdFor<T, I> = <<T as Config<I>>::TargetChain as ChainAbi>::SignerCredential;
type SignedTransactionFor<T, I> = <<T as Config<I>>::TargetChain as ChainAbi>::SignedTransaction;

benchmarks_instance_pallet! {
	on_initialize {} : {}
	// start_broadcast {
	// 	let caller: T::AccountId = whitelisted_caller();
	// 	let unsigned: SignedTransactionFor<T, I> = UnsignedTransaction {
	// 		chain_id: 42,
	// 		max_fee_per_gas: U256::from(1_000_000_000u32).into(),
	// 		gas_limit: U256::from(21_000u32).into(),
	// 		contract: [0xcf; 20].into(),
	// 		value: 0.into(),
	// 		data: b"do_something()".to_vec(),
	// 		..Default::default()
	// 	};
	// 	let call = Call::<T, I>::start_broadcast(unsigned.into());
	// 	let origin = T::EnsureWitnessed::successful_origin();
	// } : { call.dispatch_bypass_filter(origin)? }
	// transaction_ready_for_transmission {
	// 	let caller: T::AccountId = whitelisted_caller();
	// 	let broadcast_attempt_id = BroadcastAttemptId {
	// 		broadcast_id: 1,
	// 		attempt_count: 1
	// 	};
	// 	let signed_tx = SignedTransactionFor::<T, I>::default();
	// 	let signer_id = SignerIdFor::<T, I>::default();
	// } : _(RawOrigin::Signed(caller), broadcast_attempt_id, signed_tx, signer_id)
	transmission_failure {
		let origin = T::EnsureWitnessed::successful_origin();
		let transaction_hash = TransactionHashFor::<T, I>::benchmark_default();
		let broadcast_attempt_id = BroadcastAttemptId {
			broadcast_id: 1,
			attempt_count: 1
		};
		let tf = TransmissionFailure::TransactionRejected;
		let call = Call::<T, I>::transmission_failure(broadcast_attempt_id, tf, transaction_hash);
	} : { call.dispatch_bypass_filter(origin)? }
	// on_signature_ready {
	// 	let origin = T::EnsureThresholdSigned::successful_origin();
	// 	let threshold_request_id = 5;
	// 	let api_call = <T as Config<I>>::ApiCall::default();
	// 	let transmission_failure = TransmissionFailure::TransactionRejected;
	// 	let call = Call::<T, I>::on_signature_ready(threshold_request_id);
	// } : { call.dispatch_bypass_filter(origin)? }
	signature_accepted {} : {}
}
