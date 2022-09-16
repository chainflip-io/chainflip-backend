#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::{Asset, ForeignChain::Ethereum};
use frame_benchmarking::{account, benchmarks};
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};

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
			lp_account: account("doogle", 0, 0)
		});
		let call = Call::<T>::do_ingress{ingress_address: ingress_address, asset: Asset::Eth, amount: 5};
	}: {
		call.dispatch_bypass_filter(T::EnsureWitnessed::successful_origin())?;
	}
}
