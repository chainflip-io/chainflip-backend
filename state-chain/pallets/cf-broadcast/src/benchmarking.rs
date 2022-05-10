//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::eth::{TransactionHash, H256};
use frame_benchmarking::benchmarks_instance_pallet;
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};

// use cf_runtime_benchmark_utilities::BenchmarkDefault;

type TransactionHashFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::TransactionHash;

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
	transaction_ready_for_transmission {

	} : {}
	// transmission_success {
	// 	let attempt_id = 1;
	// 	let tx_hash = TransactionHashFor::<T, I>::benchmark_default();
	// 	let call = Call::<T, I>::transmission_success(attempt_id, tx_hash.into());
	// 	let origin = T::EnsureThresholdSigned::successful_origin();
	// } : {
	// 	call.dispatch_bypass_filter(origin)?
	// }
	transmission_failure {} : {}
	on_signature_ready {} : {}
	signature_accepted {} : {}
}
