#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{DisabledEgressAssets, ScheduledEgressFetchOrTransfer};
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::ForeignChain;
use frame_benchmarking::{account, benchmarks_instance_pallet};
use frame_support::traits::Hooks;

benchmarks_instance_pallet! {
	egress_assets {
		let n in 1u32 .. 254u32;
		let mut batch = vec![];

		let destination_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let destination_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		let ingress_fetch_id: <<T as Config<I>>::TargetChain as Chain>::IngressFetchId = BenchmarkValue::benchmark_value();

		// We combine fetch and egress into a single variable, assuming the weight cost is similar.
		for i in 0..n {
			if i % 2 == 0 {
				FetchParamDetails::<T, I>::insert(i as u64, (ingress_fetch_id, destination_address.clone()));
				batch.push(FetchOrTransfer::Fetch {
					channel_id: i as u64,
					asset: destination_asset,
				});
			} else {
				batch.push(FetchOrTransfer::Transfer {
					egress_id: (ForeignChain::Ethereum, i as u64),
					asset: destination_asset,
					amount: BenchmarkValue::benchmark_value(),
					destination_address: destination_address.clone(),
				});
			}
		}

		ScheduledEgressFetchOrTransfer::<T, I>::put(batch);
	} : { let _ = Pallet::<T, I>::on_idle(Default::default(), Weight::from_ref_time(1_000_000_000_000_000)); }
	verify {
		assert!(ScheduledEgressFetchOrTransfer::<T, I>::get().is_empty());
	}

	egress_ccm {
		let n in 1u32 .. 254u32;
		let mut ccms = vec![];

		let destination_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let destination_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		for i in 0..n {
			ccms.push(CrossChainMessage {
				egress_id: (ForeignChain::Ethereum, 1),
				asset: destination_asset,
				amount: BenchmarkValue::benchmark_value(),
				destination_address: destination_address.clone(),
				message: vec![0x00, 0x01, 0x02, 0x03],
				refund_address: ForeignChainAddress::Eth(Default::default()),
				source_address: ForeignChainAddress::Eth([0xcf; 20]),
			});
		}
		ScheduledEgressCcm::<T, I>::put(ccms);
	} : { let _ = Pallet::<T, I>::on_idle(Default::default(), Weight::from_ref_time(1_000_000_000_000_000)); }
	verify {
		assert!(ScheduledEgressCcm::<T, I>::get().is_empty());
	}

	disable_asset_egress {
		let origin = T::EnsureGovernance::successful_origin();
		let destination_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
	} : { let _ = Pallet::<T, I>::disable_asset_egress(origin, destination_asset, true); }
	verify {
		assert!(DisabledEgressAssets::<T, I>::get(
			destination_asset,
		).is_some());
	}

	on_idle_with_nothing_to_send {
	} : { let _ = crate::Pallet::<T, I>::on_idle(Default::default(), T::WeightInfo::egress_assets(2u32)); }

	do_single_ingress {
		let deposit_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		let ingress_amount: <<T as Config<I>>::TargetChain as Chain>::ChainAmount = BenchmarkValue::benchmark_value();
		IntentIngressDetails::<T, I>::insert(&deposit_address, IngressDetails {
				channel_id: 1,
				source_asset,
			});
		IntentActions::<T, I>::insert(&deposit_address, IntentAction::<T::AccountId>::LiquidityProvision {
			lp_account: account("doogle", 0, 0)
		});
	}: {
		Pallet::<T, I>::do_single_ingress(deposit_address, source_asset, ingress_amount, BenchmarkValue::benchmark_value()).unwrap()
	}
}
