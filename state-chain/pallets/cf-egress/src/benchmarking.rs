#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::{Asset, EthereumAddress, ForeignChain};
use frame_benchmarking::benchmarks;
use frame_support::traits::Hooks;

const ETH_ETH: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth };
const ALICE_ETH: EthereumAddress = [100u8; 20];

benchmarks! {
	send_batch_egress {
		let n in 1u32 .. 255u32;
		crate::AllowedEgressAssets::<T>::insert(ETH_ETH, ());
		let mut batch = vec![];

		for i in 0..n {
			batch.push((1_000, ForeignChainAddress::Eth(ALICE_ETH)));
		}

		crate::ScheduledEgressBatches::<T>::insert(
			ETH_ETH,
			batch,
		);
	} : { let _ = Pallet::<T>::on_idle(Default::default(), 1_000_000_000); }

	set_asset_egress_permission {
		let origin = T::EnsureGovernance::successful_origin();
	} : {let _ = Pallet::<T>::set_asset_egress_permission(origin, ETH_ETH, true);}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
