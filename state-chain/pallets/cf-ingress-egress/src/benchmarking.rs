#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{DisabledEgressAssets, FetchOrTransfer, ScheduledEgressRequests};
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::ForeignChain;
use frame_benchmarking::{account, benchmarks_instance_pallet};
use frame_support::traits::Hooks;

benchmarks_instance_pallet! {
	on_initialize {
		let n in 1u32 .. 254u32;
		let origin = T::EnsureGovernance::successful_origin();
		IntentExpiries::<T, I>::insert(T::BlockNumber::from(1_u32), (0..n as u64).map(|i| (i, TargetChainAccount::<T, I>::benchmark_value())).collect::<Vec<_>>());
		assert!(!IntentExpiries::<T, I>::get(T::BlockNumber::from(1_u32)).expect("to be in the storage").is_empty());
	} : { let _ = Pallet::<T, I>::on_initialize(T::BlockNumber::from(1_u32)); }
	verify {
		assert!(IntentExpiries::<T, I>::get(T::BlockNumber::from(1_u32)).is_none());
	}
	on_initialize_has_no_expired {
		let origin = T::EnsureGovernance::successful_origin();
	} : { let _ = Pallet::<T, I>::on_initialize(T::BlockNumber::from(1_u32)); }
	egress_assets {
		let n in 1u32 .. 254u32;
		let mut batch = vec![];

		let egress_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let egress_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		let ingress_fetch_id: <<T as Config<I>>::TargetChain as Chain>::IngressFetchId = BenchmarkValue::benchmark_value();

		// We combine fetch and egress into a single variable, assuming the weight cost is similar.
		for i in 0..n {
			if i % 2 == 0 {
				FetchParamDetails::<T, I>::insert(i as u64, (ingress_fetch_id, egress_address.clone()));
				batch.push(FetchOrTransfer::Fetch {
					intent_id: i as u64,
					asset: egress_asset,
				});
			} else {
				batch.push(FetchOrTransfer::Transfer {
					egress_id: (ForeignChain::Ethereum, i as u64),
					asset: egress_asset,
					to: egress_address.clone(),
					amount: 1_000,
				});
			}
		}

		ScheduledEgressRequests::<T, I>::put(batch);
	} : { let _ = Pallet::<T, I>::on_idle(Default::default(), Weight::from_ref_time(1_000_000_000_000_000)); }
	verify {
		assert!(ScheduledEgressRequests::<T, I>::get().is_empty());
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
	} : { let _ = crate::Pallet::<T, I>::on_idle(Default::default(), T::WeightInfo::egress_assets(2u32)); }

	do_single_ingress {
		let ingress_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let ingress_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		IntentIngressDetails::<T, I>::insert(&ingress_address, IngressDetails {
				intent_id: 1,
				ingress_asset,
			});
		IntentActions::<T, I>::insert(&ingress_address, IntentAction::<T::AccountId>::LiquidityProvision {
			lp_account: account("doogle", 0, 0)
		});
	}: {
		Pallet::<T, I>::do_single_ingress(ingress_address, ingress_asset, 100, BenchmarkValue::benchmark_value()).unwrap()
	}

	on_signature_accepted {
		let origin = T::EnsureWitnessedAtCurrentEpoch::successful_origin();
		let address = TargetChainAccount::<T, I>::benchmark_value();
		let addresses = vec![(1, TargetChainAccount::<T, I>::benchmark_value())];
		let a in 1 .. 100;
		for i in 0..a {
			IntentIngressDetails::<T, I>::insert(address.clone(), IngressDetails {
				intent_id: 1,
				ingress_asset: BenchmarkValue::benchmark_value(),
			});
			IntentActions::<T, I>::insert(address.clone(), IntentAction::<T::AccountId>::LiquidityProvision {
				lp_account: account("doogle", 0, 0)
			});
		}
	} : { let _ = Pallet::<T, I>::on_signature_accepted(origin, addresses);  }
}
