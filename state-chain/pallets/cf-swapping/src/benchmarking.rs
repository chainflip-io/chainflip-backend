//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::*;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_system::RawOrigin;

benchmarks! {
	register_swap_intent {
		let caller: T::AccountId = whitelisted_caller();
	}: _(
		RawOrigin::Signed(caller.clone()),
		ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
		ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
		ForeignChainAddress::Eth(Default::default()),
		0
	)
}
