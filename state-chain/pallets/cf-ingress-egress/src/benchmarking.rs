#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::DisabledEgressAssets;
use cf_chains::{
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	DepositChannel,
};
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::OriginTrait};

pub(crate) type TargetChainBlockNumber<T, I> =
	<<T as Config<I>>::TargetChain as Chain>::ChainBlockNumber;

#[instance_benchmarks]
mod benchmarks {
	use super::*;
	use sp_std::vec;

	#[benchmark]
	fn disable_asset_egress() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let destination_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset =
			BenchmarkValue::benchmark_value();

		#[block]
		{
			assert_ok!(Pallet::<T, I>::enable_or_disable_egress(origin, destination_asset, true));
		}

		assert!(DisabledEgressAssets::<T, I>::get(destination_asset,).is_some());
	}

	#[benchmark]
	fn process_single_deposit() {
		let deposit_address: <<T as Config<I>>::TargetChain as Chain>::ChainAccount =
			BenchmarkValue::benchmark_value();
		let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset =
			BenchmarkValue::benchmark_value();
		let deposit_amount: <<T as Config<I>>::TargetChain as Chain>::ChainAmount =
			BenchmarkValue::benchmark_value();
		let block_number: TargetChainBlockNumber<T, I> = BenchmarkValue::benchmark_value();
		DepositChannelLookup::<T, I>::insert(
			&deposit_address,
			DepositChannelDetails {
				opened_at: block_number,
				expires_at: block_number,
				deposit_channel:
					DepositChannel::generate_new::<<T as Config<I>>::AddressDerivation>(
						1,
						source_asset,
					)
					.unwrap(),
				action: ChannelAction::<T::AccountId>::LiquidityProvision {
					lp_account: account("doogle", 0, 0),
				},
				boost_fee: 0,
			},
		);

		#[block]
		{
			assert_ok!(Pallet::<T, I>::process_single_deposit(
				deposit_address,
				source_asset,
				deposit_amount,
				BenchmarkValue::benchmark_value(),
				BenchmarkValue::benchmark_value()
			));
		}
	}
	#[benchmark]
	fn finalise_ingress(a: Linear<1, 100>) {
		let mut addresses = vec![];
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		for _ in 1..a {
			let deposit_address =
				<<T as Config<I>>::TargetChain as Chain>::ChainAccount::benchmark_value_by_id(
					a as u8,
				);
			let source_asset: <<T as Config<I>>::TargetChain as Chain>::ChainAsset =
				BenchmarkValue::benchmark_value();
			let block_number = TargetChainBlockNumber::<T, I>::benchmark_value();
			let mut channel =
				DepositChannelDetails::<T, I> {
					opened_at: block_number,
					expires_at: block_number,
					deposit_channel: DepositChannel::generate_new::<
						<T as Config<I>>::AddressDerivation,
					>(1, source_asset)
					.unwrap(),
					action: ChannelAction::<T::AccountId>::LiquidityProvision {
						lp_account: account("doogle", 0, 0),
					},
					boost_fee: 0,
				};
			channel.deposit_channel.state.on_fetch_scheduled();
			DepositChannelLookup::<T, I>::insert(deposit_address.clone(), channel);
			addresses.push(deposit_address);
		}

		#[block]
		{
			assert_ok!(Pallet::<T, I>::finalise_ingress(origin, addresses));
		}
	}

	#[benchmark]
	fn vault_transfer_failed() {
		let epoch = T::EpochInfo::epoch_index();
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		let asset: TargetChainAsset<T, I> = BenchmarkValue::benchmark_value();
		let amount: TargetChainAmount<T, I> = BenchmarkValue::benchmark_value();
		let destination_address: TargetChainAccount<T, I> = BenchmarkValue::benchmark_value();

		#[block]
		{
			assert_ok!(Pallet::<T, I>::vault_transfer_failed(
				origin,
				asset,
				amount,
				destination_address.clone()
			));
		}

		assert_eq!(FailedForeignChainCalls::<T, I>::get(epoch).len(), 1);
	}

	#[benchmark]
	fn ccm_broadcast_failed() {
		#[block]
		{
			assert_ok!(Pallet::<T, I>::ccm_broadcast_failed(
				OriginTrait::root(),
				Default::default()
			));
		}

		let current_epoch = T::EpochInfo::epoch_index();
		assert_eq!(
			FailedForeignChainCalls::<T, I>::get(current_epoch),
			vec![FailedForeignChainCall {
				broadcast_id: Default::default(),
				original_epoch: current_epoch
			}]
		);
	}

	#[cfg(test)]
	use crate::mock_eth::*;

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_ccm_broadcast_failed::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_vault_transfer_failed::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_finalise_ingress::<Test, ()>(100, true);
		});
		new_test_ext().execute_with(|| {
			_process_single_deposit::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_disable_asset_egress::<Test, ()>(true);
		});
	}
}
