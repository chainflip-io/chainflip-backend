#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::{EthereumDisabledEgressAssets, EthereumScheduledEgress, EthereumScheduledIngressFetch};
use cf_primitives::{Asset, EthereumAddress, ETHEREUM_ETH_ADDRESS};
use frame_benchmarking::benchmarks;
use frame_support::traits::Hooks;

const ALICE_ETH: EthereumAddress = [100u8; 20];

benchmarks! {
	send_ethereum_batch {
		let n in 1u32 .. 255u32;
		let m in 1u32 .. 255u32;
		let mut egress_batch = vec![];
		let mut fetch_batch = vec![];

		for i in 0..n {
			fetch_batch.push(
				FetchAssetParams { swap_id: i as u64, asset: ETHEREUM_ETH_ADDRESS.into() }
			);
		}
		for i in 0..m {
			egress_batch.push(TransferAssetParams {
				asset: ETHEREUM_ETH_ADDRESS.into(),
				to: ALICE_ETH.into(),
				amount: 1_000u128
			});
		}

		EthereumScheduledEgress::<T>::set(
			egress_batch,
		);
		EthereumScheduledIngressFetch::<T>::set(
			fetch_batch,
		);
	} : { let _ = Pallet::<T>::on_idle(Default::default(), 1_000_000_000_000_000); }
	verify {
		assert!(EthereumScheduledEgress::<T>::get().is_empty());
		assert!(EthereumScheduledIngressFetch::<T>::get().is_empty());
	}

	disable_ethereum_asset_egress {
		let origin = T::EnsureGovernance::successful_origin();
	} : { let _ = Pallet::<T>::disable_ethereum_asset_egress(origin, Asset::Eth, true); }
	verify {
		assert!(EthereumDisabledEgressAssets::<T>::get(
			ETHEREUM_ETH_ADDRESS,
		).is_some());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
