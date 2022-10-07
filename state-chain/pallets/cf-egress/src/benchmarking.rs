#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::{DisabledEgressAssets, ScheduledEgress};
use cf_primitives::{Asset, EthereumAddress, ForeignChain};
use frame_benchmarking::benchmarks;
use frame_support::traits::Hooks;

const ETH_ETH: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth };
const ALICE_ETH: EthereumAddress = [100u8; 20];

benchmarks! {
	send_batch_egress {
		let n in 1u32 .. 255u32;
		let m in 1u32 .. 255u32;
		let mut egress_batch = vec![];
		let mut fetch_batch = vec![];

		for i in 0..n {
			egress_batch.push((1_000, ForeignChainAddress::Eth(ALICE_ETH)));
		}
		for i in 0..m {
			fetch_batch.push(i as u64);
		}

		ScheduledEgress::<T>::insert(
			ETH_ETH,
			egress_batch,
		);
		EthereumScheduledIngressFetch::<T>::insert(
			Asset::Eth,
			fetch_batch,
		);
	} : { let _ = Pallet::<T>::on_idle(Default::default(), 1_000_000_000_000_000); }
	verify {
		assert!(ScheduledEgress::<T>::get(
			ETH_ETH,
		).is_empty());
	}

	disable_asset_egress {
		let origin = T::EnsureGovernance::successful_origin();
	} : { let _ = Pallet::<T>::disable_asset_egress(origin, ETH_ETH, true); }
	verify {
		assert!(DisabledEgressAssets::<T>::get(
			ETH_ETH,
		).is_some());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
