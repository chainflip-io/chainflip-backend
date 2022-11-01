#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::{EthereumDisabledEgressAssets, EthereumRequest, EthereumScheduledRequests};
use cf_primitives::{Asset, EthereumAddress, ForeignChain};
use frame_benchmarking::benchmarks;
use frame_support::traits::Hooks;

const ETH_ETH: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth };
const ALICE_ETH: EthereumAddress = [100u8; 20];

benchmarks! {
	send_ethereum_batch {
		let n in 1u32 .. 254u32;
		let mut batch = vec![];

		// We combine fetch and egress into a single variable, assuming the weight cost is similar.
		for i in 0..n {
			if i%2==0 {
				batch.push(EthereumRequest::Fetch {
					intent_id: 1,
					asset: Asset::Eth,
				});
			} else {
				batch.push(EthereumRequest::Transfer {
					asset: Asset::Eth,
					to: ALICE_ETH,
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
	} : { let _ = Pallet::<T>::disable_asset_egress(origin, ETH_ETH, true); }
	verify {
		assert!(EthereumDisabledEgressAssets::<T>::get(
			Asset::Eth,
		).is_some());
	}

	on_idle_with_nothing_to_send {
	} : { let _ = crate::Pallet::<T>::on_idle(Default::default(), T::WeightInfo::send_ethereum_batch(2u32)); }

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
