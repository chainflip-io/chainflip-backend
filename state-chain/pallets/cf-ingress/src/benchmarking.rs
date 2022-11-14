#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_chains::benchmarking_value::BenchmarkValue;
use frame_benchmarking::{account, benchmarks_instance_pallet};
use frame_support::sp_runtime::app_crypto::sp_core;
use sp_core::H256;

benchmarks_instance_pallet! {
	where_clause {
		where
		T: Config<I>,
		<<T as Config<I>>::TargetChain as Chain>::ChainAsset: Into<cf_primitives::Asset>,
		<<T as Config<I>>::TargetChain as Chain>::ChainAccount:
			TryFrom<cf_primitives::ForeignChainAddress>,
	}

	do_single_ingress {
		let ingress_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let ingress_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		IntentIngressDetails::<T, I>::insert(&ingress_address, IngressDetails {
				intent_id: 1,
				ingress_asset,
			});
		IntentActions::<T, I>::insert(&ingress_address, IntentAction::<T::AccountId>::LiquidityProvision {
			lp_account: account("doogle", 0, 0)
		});
	}: {
		Pallet::<T, I>::do_single_ingress(ingress_address, ingress_asset, 100, H256::from([0x01; 32])).unwrap()
	}
}
