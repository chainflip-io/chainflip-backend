#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::{EthereumDisabledEgressAssets, EthereumScheduledEgress};
use cf_primitives::{Asset, EthereumAddress, ForeignChain};
use frame_benchmarking::benchmarks;
use frame_support::traits::Hooks;

const ETH_ETH: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth };
const ALICE_ETH: EthereumAddress = [100u8; 20];

benchmarks! {
	send_ethereum_batch {
		let n in 1u32 .. 255u32;
		let m in 1u32 .. 255u32;
		let mut egress_batch = vec![];
		let mut fetch_batch = vec![];

		for i in 0..n {
			fetch_batch.push(i as u64);
		}
		for i in 0..m {
			egress_batch.push((1_000, ALICE_ETH));
		}

		EthereumScheduledEgress::<T>::insert(
			Asset::Eth,
			egress_batch,
		);
		EthereumScheduledIngressFetch::<T>::insert(
			Asset::Eth,
			fetch_batch,
		);
	} : { let _ = Pallet::<T>::on_idle(Default::default(), 1_000_000_000_000_000); }
	verify {
		assert!(EthereumScheduledEgress::<T>::get(
			Asset::Eth,
		).is_empty());
	}

	disable_asset_egress {
		let origin = T::EnsureGovernance::successful_origin();
	} : { let _ = Pallet::<T>::disable_asset_egress(origin, ETH_ETH, true); }
	verify {
		assert!(EthereumDisabledEgressAssets::<T>::get(
			Asset::Eth,
		).is_some());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
