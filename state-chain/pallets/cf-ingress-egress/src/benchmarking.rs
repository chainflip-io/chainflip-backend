#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{DisabledEgressAssets, ScheduledEgressFetchOrTransfer};
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::ForeignChain;
use frame_benchmarking::{account, benchmarks_instance_pallet};
use frame_support::traits::Hooks;

benchmarks_instance_pallet! {
	destination_assets {
		let n in 1u32 .. 254u32;
		let mut batch = vec![];

		let destination_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let destination_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		let deposit_fetch_id: <<T as Config<I>>::TargetChain as Chain>::DepositFetchId = BenchmarkValue::benchmark_value();

		// We combine fetch and egress into a single variable, assuming the weight cost is similar.
		for i in 0..n {
			if i % 2 == 0 {
				FetchParamDetails::<T, I>::insert(i as u64, (deposit_fetch_id, destination_address.clone()));
				AddressStatus::<T, I>::insert(destination_address.clone(), DeploymentStatus::Deployed);
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
	} : { let _ = crate::Pallet::<T, I>::on_idle(Default::default(), T::WeightInfo::destination_assets(2u32)); }

	process_single_deposit {
		let deposit_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		let deposit_amount: <<T as Config<I>>::TargetChain as Chain>::ChainAmount = BenchmarkValue::benchmark_value();
		DepositAddressDetailsLookup::<T, I>::insert(&deposit_address, DepositAddressDetails {
				channel_id: 1,
				source_asset,
			});
		ChannelActions::<T, I>::insert(&deposit_address, ChannelAction::<T::AccountId>::LiquidityProvision {
			lp_account: account("doogle", 0, 0)
		});
	}: {
		Pallet::<T, I>::process_single_deposit(deposit_address, source_asset, deposit_amount, BenchmarkValue::benchmark_value()).unwrap()
	}

	set_minimum_deposit {
		let origin = T::EnsureGovernance::successful_origin();
		let destination_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		let amount: <<T as Config<I>>::TargetChain as Chain>::ChainAmount =  BenchmarkValue::benchmark_value();
	} : { let _ = Pallet::<T, I>::set_minimum_deposit(origin, destination_asset, amount); }
	verify {
		assert_eq!(MinimumDeposit::<T, I>::get(
			destination_asset,
		).into(), 1_000u128);
	}

	finalise_ingress {
		let a in 1 .. 100;
		let mut addresses = vec![];
		let origin = T::EnsureWitnessedAtCurrentEpoch::successful_origin();
		let deposit_fetch_id: <<T as Config<I>>::TargetChain as Chain>::DepositFetchId = BenchmarkValue::benchmark_value();
		let deposit_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		for i in 1..a {
			// TODO: Thats wrong, we need to insert different addresses, otherwise we will overwrite the same one amd thats not the expensive path.
			// Unfortunately we can not so easily generate different addresses in an benchmark environment...
			AddressStatus::<T, I>::insert(deposit_address.clone(), DeploymentStatus::Pending);
			addresses.push((deposit_fetch_id, deposit_address.clone()));
		}
	}: { let _ = Pallet::<T, I>::finalise_ingress(origin, addresses); }
}
