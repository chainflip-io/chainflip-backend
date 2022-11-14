#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::{EthereumDisabledEgressAssets, EthereumScheduledRequests, FetchOrTransfer};
use cf_primitives::{chains::assets::eth, EthereumAddress};
use frame_benchmarking::benchmarks;
use frame_support::traits::Hooks;

const ALICE_ETH: EthereumAddress = [100u8; 20];

benchmarks! {
	send_ethereum_batch {
		let n in 1u32 .. 254u32;
		let mut batch = vec![];

		// We combine fetch and egress into a single variable, assuming the weight cost is similar.
		for i in 0..n {
			if i%2==0 {
				batch.push(FetchOrTransfer::<Ethereum>::Fetch {
					intent_id: 1,
					asset: eth::Asset::Eth,
				});
			} else {
				batch.push(FetchOrTransfer::<Ethereum>::Transfer {
					asset: eth::Asset::Eth,
					to: ALICE_ETH.into(),
					amount: 1_000,
				});
			}
		}

		EthereumScheduledRequests::<T>::put(batch);
	} : { let _ = Pallet::<T>::on_idle(Default::default(), 1_000_000_000_000_000); }
	verify {
		assert!(EthereumScheduledRequests::<T>::get().is_empty());
	}

	disable_asset_egress {
		let origin = T::EnsureGovernance::successful_origin();
	} : { let _ = Pallet::<T>::disable_asset_egress(origin, eth::Asset::Eth, true); }
	verify {
		assert!(EthereumDisabledEgressAssets::<T>::get(
			eth::Asset::Eth,
		).is_some());
	}

	on_idle_with_nothing_to_send {
	} : { let _ = crate::Pallet::<T>::on_idle(Default::default(), T::WeightInfo::send_ethereum_batch(2u32)); }

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
