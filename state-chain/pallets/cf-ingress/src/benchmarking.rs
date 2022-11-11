#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::Asset;
use frame_benchmarking::{account, benchmarks_instance_pallet};
use frame_support::sp_runtime::app_crypto::sp_core;
use sp_core::H256;

benchmarks_instance_pallet! {
	do_single_ingress {
		let ingress_address = ForeignChainAddress::Eth([0; 20]);
		let ingress_asset = Asset::Eth;
		IntentIngressDetails::<T, I>::insert(ingress_address, IngressDetails {
				intent_id: 1,
				ingress_asset,
			});
		IntentActions::<T, I>::insert(ingress_address, IntentAction::<T::AccountId>::LiquidityProvision {
			lp_account: account("doogle", 0, 0)
		});
	}: {
		Pallet::<T, I>::do_single_ingress(ingress_address, ingress_asset, 100, H256::from([0x01; 32])).unwrap()
	}
}
