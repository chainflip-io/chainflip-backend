//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::*;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_system::RawOrigin;

benchmarks! {
	register_swap_intent {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Relayer);
	}: _(
		RawOrigin::Signed(caller.clone()),
		ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
		ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
		ForeignChainAddress::Eth(Default::default()),
		0
	)
	execute_swap {
		let swap = Swap { from: Asset::Eth, to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc }, amount: 10, ingress_address: ForeignChainAddress::Eth(Default::default()), egress_address: ForeignChainAddress::Eth(Default::default())};
	}: {
		Pallet::<T>::execute_swap(swap);
	}
	on_idle {}: {
		Pallet::<T>::on_idle(T::BlockNumber::from(1u32), 1);
	}
}
