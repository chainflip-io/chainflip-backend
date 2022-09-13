#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::{Asset, ForeignChain::Ethereum};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};
use frame_system::RawOrigin;

benchmarks! {
	do_ingress {
		let ingress_address = ForeignChainAddress::Eth([0; 20]);
		OpenIntents::<T>::insert(ingress_address, Intent::<T::AccountId>::LiquidityProvision {
			ingress_details:  IngressDetails {
				intent_id: 1,
				ingress_asset: ForeignChainAsset {
					chain: Ethereum,
					asset: Asset::Eth,
				}
			},
			lp_account: whitelisted_caller()
		});
		let call = Call::<T>::do_ingress{ingress_address: ingress_address, asset: Asset::Eth, amount: 5};
	}: {
		call.dispatch_bypass_filter(T::EnsureWitnessed::successful_origin())?;
	}
	register_liquidity_ingress_intent_temp {
		let caller: T::AccountId = whitelisted_caller();
		let foreign_chain_asset = ForeignChainAsset {
			chain: Ethereum,
			asset: Asset::Eth,
		};
	}: _(RawOrigin::Signed(caller.clone()), foreign_chain_asset)
}
