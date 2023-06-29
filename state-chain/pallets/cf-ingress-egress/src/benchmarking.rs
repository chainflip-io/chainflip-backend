#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::DisabledEgressAssets;
use cf_chains::benchmarking_value::{BenchmarkValue, BenchmarkValueExtended};
use frame_benchmarking::{account, benchmarks_instance_pallet};

benchmarks_instance_pallet! {
	disable_asset_egress {
		let origin = T::EnsureGovernance::successful_origin();
		let destination_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
	} : { let _ = Pallet::<T, I>::disable_asset_egress(origin, destination_asset, true); }
	verify {
		assert!(DisabledEgressAssets::<T, I>::get(
			destination_asset,
		).is_some());
	}

	process_single_deposit {
		let deposit_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		let deposit_amount: <<T as Config<I>>::TargetChain as Chain>::ChainAmount = BenchmarkValue::benchmark_value();
		DepositAddressDetailsLookup::<T, I>::insert(&deposit_address, (DepositAddressDetails {
				channel_id: 1,
				source_asset,
			}, <T::TargetChain as Chain>::DepositAddress::new(
				1,
				deposit_address.clone(),
			)));
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
		), amount);
	}

	finalise_ingress {
		let a in 1 .. 100;
		let mut addresses = vec![];
		let origin = T::EnsureWitnessedAtCurrentEpoch::successful_origin();
		for i in 1..a {
			let deposit_address = <<T as Config<I>>::TargetChain as Chain>::ChainAccount::benchmark_value_by_id(a as u8);
			let deposit_fetch_id = <<T as Config<I>>::TargetChain as Chain>::DepositFetchId::benchmark_value_by_id(a as u8);
			let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
			DepositAddressDetailsLookup::<T, I>::insert(deposit_address.clone(), (DepositAddressDetails {
				channel_id: a as u64,
				source_asset,
			}, <T::TargetChain as Chain>::DepositAddress::new(
				a as u64,
				deposit_address.clone(),
			)));
			addresses.push((deposit_fetch_id, deposit_address));
		}
	}: { let _ = Pallet::<T, I>::finalise_ingress(origin, addresses); }

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
