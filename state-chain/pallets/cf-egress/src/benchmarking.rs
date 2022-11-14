#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{DisabledEgressAssets, FetchOrTransfer, ScheduledRequests};
use cf_chains::benchmarking_value::BenchmarkValue;
use frame_benchmarking::benchmarks_instance_pallet;
use frame_support::traits::Hooks;

benchmarks_instance_pallet! {
	send_batch {
		let n in 1u32 .. 254u32;
		let mut batch = vec![];

		let egress_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let egress_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();

		// We combine fetch and egress into a single variable, assuming the weight cost is similar.
		for i in 0..n {
			if i % 2 == 0 {
				batch.push(FetchOrTransfer::Fetch {
					intent_id: 1,
					asset: egress_asset,
				});
			} else {
				batch.push(FetchOrTransfer::Transfer {
					asset: egress_asset,
					to: egress_address.clone(),
					amount: 1_000,
				});
			}
		}

		ScheduledRequests::<T, I>::put(batch);
	} : { let _ = Pallet::<T, I>::on_idle(Default::default(), 1_000_000_000_000_000); }
	verify {
		assert!(ScheduledRequests::<T, I>::get().is_empty());
	}

	disable_asset_egress {
		let origin = T::EnsureGovernance::successful_origin();
		let egress_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
	} : { let _ = Pallet::<T, I>::disable_asset_egress(origin, egress_asset, true); }
	verify {
		assert!(DisabledEgressAssets::<T, I>::get(
			egress_asset,
		).is_some());
	}

	on_idle_with_nothing_to_send {
	} : { let _ = crate::Pallet::<T, I>::on_idle(Default::default(), T::WeightInfo::send_batch(2u32)); }

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
