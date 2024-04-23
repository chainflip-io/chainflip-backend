#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{BoostStatus, DisabledEgressAssets};
use cf_chains::{
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	DepositChannel,
};
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{OnNewAccount, OriginTrait},
};
use frame_system::RawOrigin;
use strum::IntoEnumIterator;

pub(crate) type TargetChainBlockNumber<T, I> =
	<<T as Config<I>>::TargetChain as Chain>::ChainBlockNumber;

#[instance_benchmarks]
mod benchmarks {
	use super::*;

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
		const CHANNEL_ID: u64 = 1;
		const PREWITNESSED_DEPOSIT_ID: u64 = 1;

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
						CHANNEL_ID,
						source_asset,
					)
					.unwrap(),
				action: ChannelAction::<T::AccountId>::LiquidityProvision {
					lp_account: account("doogle", 0, 0),
				},
				boost_fee: 0,
				boost_status: BoostStatus::NotBoosted,
			},
		);
		PrewitnessedDeposits::<T, I>::insert(
			CHANNEL_ID,
			PREWITNESSED_DEPOSIT_ID,
			PrewitnessedDeposit {
				asset: source_asset,
				amount: deposit_amount,
				deposit_address: deposit_address.clone(),
				deposit_details: BenchmarkValue::benchmark_value(),
				block_height: BenchmarkValue::benchmark_value(),
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

		assert!(PrewitnessedDeposits::<T, I>::get(CHANNEL_ID, PREWITNESSED_DEPOSIT_ID).is_none());
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
					boost_status: BoostStatus::NotBoosted,
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

	fn setup_booster_account<T: Config<I>, I>(
		asset: TargetChainAsset<T, I>,
		seed: u32,
	) -> T::AccountId {
		let caller: T::AccountId = account("booster", 0, seed);

		// TODO: remove once https://github.com/chainflip-io/chainflip-backend/pull/4716 is merged
		if frame_system::Pallet::<T>::providers(&caller) == 0u32 {
			frame_system::Pallet::<T>::inc_providers(&caller);
		}
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller));
		assert_ok!(T::LpBalance::try_credit_account(&caller, asset.into(), 1_000_000,));

		// A non-zero balance is required to pay for the channel opening fee.
		T::FeePayment::mint_to_account(&caller, u32::MAX.into());

		assert_ok!(T::LpBalance::try_credit_account(
			&caller,
			asset.into(),
			5_000_000_000_000_000_000u128
		));

		caller
	}

	#[benchmark]
	fn add_boost_funds() {
		use strum::IntoEnumIterator;
		let amount: TargetChainAmount<T, I> = 1000u32.into();

		const FEE_TIER: BoostPoolTier = BoostPoolTier::FiveBps;
		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		let lp_account = setup_booster_account::<T, I>(asset, 0);

		#[block]
		{
			assert_ok!(Pallet::<T, I>::add_boost_funds(
				RawOrigin::Signed(lp_account.clone()).into(),
				asset,
				amount,
				FEE_TIER
			));
		}

		assert_eq!(
			BoostPools::<T, I>::get(asset, FEE_TIER).unwrap().get_available_amount(),
			amount
		);
	}

	fn prewitness_deposit<T: pallet::Config<I>, I>(
		lp_account: &T::AccountId,
		asset: TargetChainAsset<T, I>,
		fee_tier: BoostPoolTier,
	) -> TargetChainAccount<T, I> {
		let (_channel_id, deposit_address, ..) = Pallet::<T, I>::open_channel(
			lp_account,
			asset,
			ChannelAction::LiquidityProvision { lp_account: lp_account.clone() },
			fee_tier as u16,
		)
		.unwrap();

		assert_ok!(Pallet::<T, I>::add_prewitnessed_deposits(
			vec![DepositWitness::<T::TargetChain> {
				deposit_address: deposit_address.clone(),
				asset,
				amount: TargetChainAmount::<T, I>::from(1000u32),
				deposit_details: BenchmarkValue::benchmark_value()
			}],
			BenchmarkValue::benchmark_value()
		),);

		deposit_address
	}

	#[benchmark]
	fn on_lost_deposit(n: Linear<1, 100>) {
		const FEE_TIER: BoostPoolTier = BoostPoolTier::FiveBps;
		use strum::IntoEnumIterator;
		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		let boosters: Vec<_> = (0..n).map(|i| setup_booster_account::<T, I>(asset, i)).collect();

		for booster_id in &boosters {
			assert_ok!(Pallet::<T, I>::add_boost_funds(
				RawOrigin::Signed(booster_id.clone()).into(),
				asset,
				1_000_000u32.into(),
				FEE_TIER
			));
		}

		prewitness_deposit::<T, I>(&boosters[0], asset, FEE_TIER);

		// Worst-case scenario is when all boosters withdraw funds while
		// waiting for the deposit to be finalised:
		for booster_id in &boosters {
			assert_ok!(Pallet::<T, I>::stop_boosting(
				RawOrigin::Signed(booster_id.clone()).into(),
				asset,
				FEE_TIER
			));
		}

		let prewitnessed_deposit_id = PrewitnessedDepositIdCounter::<T, I>::get();

		#[block]
		{
			BoostPools::<T, I>::mutate(asset, FEE_TIER, |pool| {
				// This depends on the number of boosters who contributed to it:
				pool.as_mut().unwrap().on_lost_deposit(prewitnessed_deposit_id);
			});
		}
	}

	#[benchmark]
	fn stop_boosting() {
		const FEE_TIER: BoostPoolTier = BoostPoolTier::FiveBps;
		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		let lp_account = setup_booster_account::<T, I>(asset, 0);

		assert_ok!(Pallet::<T, I>::add_boost_funds(
			RawOrigin::Signed(lp_account.clone()).into(),
			asset,
			1_000_000u32.into(),
			FEE_TIER
		));

		// `stop_boosting` has linear complexity w.r.t. the number of pending boosts,
		// and this seems like a reasonable estimate:
		const PENDING_BOOSTS_COUNT: usize = 50;

		for _ in 0..PENDING_BOOSTS_COUNT {
			prewitness_deposit::<T, I>(&lp_account, asset, FEE_TIER);
		}

		#[block]
		{
			// This depends on the number active boosts:
			assert_ok!(Pallet::<T, I>::stop_boosting(
				RawOrigin::Signed(lp_account).into(),
				asset,
				FEE_TIER
			));
		}

		assert_eq!(
			BoostPools::<T, I>::get(asset, FEE_TIER).unwrap().get_available_amount(),
			0u32.into()
		);
	}

	// This benchmark is currently not used (since we use the more computationally expensive
	// boost_finalised instead), but it is useful to keep around even if just to show that
	// boosting a deposit is relatively cheap.
	#[benchmark]
	fn deposit_boosted() {
		const FEE_TIER: BoostPoolTier = BoostPoolTier::FiveBps;
		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		const BOOSTER_COUNT: u32 = 100;

		let boosters: Vec<_> =
			(0..BOOSTER_COUNT).map(|i| setup_booster_account::<T, I>(asset, i)).collect();

		for booster_id in &boosters {
			assert_ok!(Pallet::<T, I>::add_boost_funds(
				RawOrigin::Signed(booster_id.clone()).into(),
				asset,
				1_000_000u32.into(),
				FEE_TIER
			));
		}

		let (_channel_id, deposit_address, ..) = Pallet::<T, I>::open_channel(
			&boosters[0],
			asset,
			ChannelAction::LiquidityProvision { lp_account: boosters[0].clone() },
			FEE_TIER as u16,
		)
		.unwrap();

		let amount_before =
			BoostPools::<T, I>::get(asset, FEE_TIER).unwrap().get_available_amount();

		#[block]
		{
			assert_ok!(Pallet::<T, I>::add_prewitnessed_deposits(
				vec![DepositWitness::<T::TargetChain> {
					deposit_address,
					asset,
					amount: TargetChainAmount::<T, I>::from(1000u32),
					deposit_details: BenchmarkValue::benchmark_value()
				}],
				BenchmarkValue::benchmark_value()
			),);
		}

		// This would fail if the deposit didn't get boosted:
		assert!(
			BoostPools::<T, I>::get(asset, FEE_TIER).unwrap().get_available_amount() <
				amount_before
		)
	}

	#[benchmark]
	fn boost_finalised() {
		use sp_runtime::Percent;
		const FEE_TIER: BoostPoolTier = BoostPoolTier::FiveBps;
		use strum::IntoEnumIterator;
		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		const BOOSTER_COUNT: usize = 100;

		let boosters: Vec<_> = (0..BOOSTER_COUNT)
			.map(|i| setup_booster_account::<T, I>(asset, i as u32))
			.collect();

		for booster_id in &boosters {
			assert_ok!(Pallet::<T, I>::add_boost_funds(
				RawOrigin::Signed(booster_id.clone()).into(),
				asset,
				1_000_000u32.into(),
				FEE_TIER
			));
		}

		let deposit_address = prewitness_deposit::<T, I>(&boosters[0], asset, FEE_TIER);

		// Finalisation is more expensive the more boosters are withdrawing, as that requires
		// storage access to update their balances for each withdrawing booster. It is overly
		// pessimistic to assume all will be withdrawing, so we assume only `PERCENT_WITHDRAWING`
		// do so:
		const PERCENT_WITHDRAWING: Percent = Percent::from_percent(30);
		let withdrawing_count: usize = PERCENT_WITHDRAWING * BOOSTER_COUNT;

		for booster_id in boosters.iter().take(withdrawing_count) {
			assert_ok!(Pallet::<T, I>::stop_boosting(
				RawOrigin::Signed(booster_id.clone()).into(),
				asset,
				FEE_TIER
			));
		}

		let amount_before =
			BoostPools::<T, I>::get(asset, FEE_TIER).unwrap().get_available_amount();

		#[block]
		{
			assert_ok!(Pallet::<T, I>::process_single_deposit(
				deposit_address,
				asset,
				1_000u32.into(),
				BenchmarkValue::benchmark_value(),
				BenchmarkValue::benchmark_value()
			));
		}

		// Balance should increase due to boost finalisation:
		assert!(
			BoostPools::<T, I>::get(asset, FEE_TIER).unwrap().get_available_amount() >
				amount_before
		)
	}

	#[benchmark]
	fn clear_prewitnessed_deposits(n: Linear<1, 255>) {
		for i in 0..n {
			PrewitnessedDeposits::<T, I>::insert(
				0,
				i as u64,
				PrewitnessedDeposit {
					asset: BenchmarkValue::benchmark_value(),
					amount: BenchmarkValue::benchmark_value(),
					deposit_address: BenchmarkValue::benchmark_value(),
					deposit_details: BenchmarkValue::benchmark_value(),
					block_height: BenchmarkValue::benchmark_value(),
				},
			);
		}

		#[block]
		{
			assert_eq!(Pallet::<T, I>::clear_prewitnessed_deposits(0), n as u32);
		}

		assert_eq!(PrewitnessedDeposits::<T, I>::iter().count(), 0);
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
		new_test_ext().execute_with(|| {
			_clear_prewitnessed_deposits::<Test, ()>(100, true);
		});
		new_test_ext().execute_with(|| {
			_add_boost_funds::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_on_lost_deposit::<Test, ()>(100, true);
		});
		new_test_ext().execute_with(|| {
			_stop_boosting::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_deposit_boosted::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_boost_finalised::<Test, ()>(true);
		});
	}
}
