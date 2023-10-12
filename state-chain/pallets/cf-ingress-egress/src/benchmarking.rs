#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::DisabledEgressAssets;
use cf_chains::{
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	DepositChannel,
};
use frame_benchmarking::{account, benchmarks_instance_pallet};

pub(crate) type TargetChainBlockNumber<T, I> =
	<<T as Config<I>>::TargetChain as Chain>::ChainBlockNumber;

benchmarks_instance_pallet! {
	disable_asset_egress {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let destination_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
	} : { let _ = Pallet::<T, I>::enable_or_disable_egress(origin, destination_asset, true); }
	verify {
		assert!(DisabledEgressAssets::<T, I>::get(
			destination_asset,
		).is_some());
	}

	process_single_deposit {
		let deposit_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount = BenchmarkValue::benchmark_value();
		let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
		let deposit_amount: <<T as Config<I>>::TargetChain as Chain>::ChainAmount = BenchmarkValue::benchmark_value();
		let block_number: TargetChainBlockNumber<T, I> = BenchmarkValue::benchmark_value();
		DepositChannelLookup::<T, I>::insert(&deposit_address, DepositChannelDetails {
			opened_at: block_number,
			expires_at: block_number,
			deposit_channel: DepositChannel::generate_new::<<T as Config<I>>::AddressDerivation>(
				1,
				source_asset,
			).unwrap(),
			action: ChannelAction::<T::AccountId>::LiquidityProvision {
				lp_account: account("doogle", 0, 0),
			},
		});
	}: {
		Pallet::<T, I>::process_single_deposit(deposit_address, source_asset, deposit_amount, BenchmarkValue::benchmark_value(), BenchmarkValue::benchmark_value()).unwrap()
	}

	set_minimum_deposit {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
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
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		for i in 1..a {
			let deposit_address = <<T as Config<I>>::TargetChain as Chain>::ChainAccount::benchmark_value_by_id(a as u8);
			let deposit_fetch_id = <<T as Config<I>>::TargetChain as Chain>::DepositFetchId::benchmark_value_by_id(a as u8);
			let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset = BenchmarkValue::benchmark_value();
			let block_number = TargetChainBlockNumber::<T, I>::benchmark_value();
			let mut channel = DepositChannelDetails::<T, I> {
				opened_at: block_number,
				expires_at: block_number,
				deposit_channel: DepositChannel::generate_new::<<T as Config<I>>::AddressDerivation>(
					1,
					source_asset,
				).unwrap(),
				action: ChannelAction::<T::AccountId>::LiquidityProvision {
					lp_account: account("doogle", 0, 0),
				},
			};
			channel.deposit_channel.state.on_fetch_scheduled();
			DepositChannelLookup::<T, I>::insert(deposit_address.clone(), channel);
			addresses.push(deposit_address);
		}
	}: { let _ = Pallet::<T, I>::finalise_ingress(origin, addresses); }

	vault_transfer_failed {
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		let asset: TargetChainAsset<T, I> = BenchmarkValue::benchmark_value();
		let amount: TargetChainAmount<T, I> = BenchmarkValue::benchmark_value();
		let destination_address: TargetChainAccount<T, I> = BenchmarkValue::benchmark_value();
	}: { let _ = Pallet::<T, I>::vault_transfer_failed(origin, asset, amount, destination_address.clone()); }
	verify {
		assert_eq!(FailedVaultTransfers::<T, I>::get(),
		vec![VaultTransfer {
			asset, amount, destination_address,
		}]);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
