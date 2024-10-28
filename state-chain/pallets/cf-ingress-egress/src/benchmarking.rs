#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{BoostStatus, DisabledEgressAssets};
use cf_chains::{
	address::EncodedAddress,
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	DepositChannel,
};
use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{OnNewAccount, OriginTrait, UnfilteredDispatchable},
};
use frame_system::RawOrigin;
use strum::IntoEnumIterator;

pub(crate) type TargetChainBlockNumber<T, I> =
	<<T as Config<I>>::TargetChain as Chain>::ChainBlockNumber;

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	const TIER_5_BPS: BoostPoolTier = 5;

	fn create_boost_pool<T: pallet::Config<I>, I: 'static>() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		assert_ok!(Pallet::<T, I>::create_boost_pools(
			origin,
			vec![BoostPoolId { asset: BenchmarkValue::benchmark_value(), tier: TIER_5_BPS }]
		));
	}

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
				owner: account("doogle", 0, 0),
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
					refund_address: None,
				},
				boost_fee: 0,
				boost_status: BoostStatus::NotBoosted,
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
					owner: account("doogle", 0, 0),
					opened_at: block_number,
					expires_at: block_number,
					deposit_channel: DepositChannel::generate_new::<
						<T as Config<I>>::AddressDerivation,
					>(1, source_asset)
					.unwrap(),
					action: ChannelAction::<T::AccountId>::LiquidityProvision {
						lp_account: account("doogle", 0, 0),
						refund_address: None,
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
		assert_ok!(T::Balance::try_credit_account(&caller, asset.into(), 1_000_000,));

		// A non-zero balance is required to pay for the channel opening fee.
		T::FeePayment::mint_to_account(&caller, u32::MAX.into());

		assert_ok!(T::Balance::try_credit_account(
			&caller,
			asset.into(),
			5_000_000_000_000_000_000u128
		));

		caller
	}

	#[benchmark]
	fn add_boost_funds() {
		create_boost_pool::<T, I>();

		use strum::IntoEnumIterator;
		let amount: TargetChainAmount<T, I> = 1000u32.into();

		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		let lp_account = setup_booster_account::<T, I>(asset, 0);

		#[block]
		{
			assert_ok!(Pallet::<T, I>::add_boost_funds(
				RawOrigin::Signed(lp_account.clone()).into(),
				asset,
				amount,
				TIER_5_BPS
			));
		}

		assert_eq!(
			BoostPools::<T, I>::get(asset, TIER_5_BPS).unwrap().get_available_amount(),
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
			ChannelAction::LiquidityProvision {
				lp_account: lp_account.clone(),
				refund_address: None,
			},
			fee_tier,
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
	fn process_deposit_as_lost(n: Linear<1, 100>) {
		create_boost_pool::<T, I>();

		use strum::IntoEnumIterator;
		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		let boosters: Vec<_> = (0..n).map(|i| setup_booster_account::<T, I>(asset, i)).collect();

		for booster_id in &boosters {
			assert_ok!(Pallet::<T, I>::add_boost_funds(
				RawOrigin::Signed(booster_id.clone()).into(),
				asset,
				1_000_000u32.into(),
				TIER_5_BPS
			));
		}

		prewitness_deposit::<T, I>(&boosters[0], asset, TIER_5_BPS);

		// Worst-case scenario is when all boosters withdraw funds while
		// waiting for the deposit to be finalised:
		for booster_id in &boosters {
			assert_ok!(Pallet::<T, I>::stop_boosting(
				RawOrigin::Signed(booster_id.clone()).into(),
				asset,
				TIER_5_BPS
			));
		}

		let prewitnessed_deposit_id = PrewitnessedDepositIdCounter::<T, I>::get();

		#[block]
		{
			BoostPools::<T, I>::mutate(asset, TIER_5_BPS, |pool| {
				// This depends on the number of boosters who contributed to it:
				pool.as_mut().unwrap().process_deposit_as_lost(prewitnessed_deposit_id);
			});
		}
	}

	#[benchmark]
	fn contract_swap_request() {
		let deposit_amount = 1_000u32;

		let witness_origin = T::EnsureWitnessed::try_successful_origin().unwrap();
		let call = Call::<T, I>::contract_swap_request {
			input_asset: Asset::Usdc.try_into().unwrap(),
			output_asset: Asset::Eth,
			deposit_amount: deposit_amount.into(),
			destination_address: EncodedAddress::benchmark_value(),
			deposit_metadata: None,
			tx_hash: [0; 32],
			deposit_details: Box::new(BenchmarkValue::benchmark_value()),
			broker_fees: Default::default(),
			refund_params: None,
			dca_params: None,
			boost_fee: 0,
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(witness_origin));
		}
	}

	#[benchmark]
	fn contract_ccm_swap_request() {
		let origin = T::EnsureWitnessed::try_successful_origin().unwrap();
		let deposit_metadata = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::benchmark_value()),
			channel_metadata: CcmChannelMetadata {
				message: vec![0x00].try_into().unwrap(),
				gas_budget: 1,
				cf_parameters: Default::default(),
			},
		};
		let call = Call::<T, I>::contract_swap_request {
			input_asset: BenchmarkValue::benchmark_value(),
			deposit_amount: 1_000u32.into(),
			output_asset: Asset::Eth,
			destination_address: EncodedAddress::benchmark_value(),
			deposit_metadata: Some(deposit_metadata),
			tx_hash: Default::default(),
			deposit_details: Box::new(BenchmarkValue::benchmark_value()),
			broker_fees: Default::default(),
			refund_params: None,
			dca_params: None,
			boost_fee: 0,
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}
	}

	#[benchmark]
	fn stop_boosting() {
		create_boost_pool::<T, I>();

		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		let lp_account = setup_booster_account::<T, I>(asset, 0);

		assert_ok!(Pallet::<T, I>::add_boost_funds(
			RawOrigin::Signed(lp_account.clone()).into(),
			asset,
			1_000_000u32.into(),
			TIER_5_BPS
		));

		// `stop_boosting` has linear complexity w.r.t. the number of pending boosts,
		// and this seems like a reasonable estimate:
		const PENDING_BOOSTS_COUNT: usize = 50;

		for _ in 0..PENDING_BOOSTS_COUNT {
			prewitness_deposit::<T, I>(&lp_account, asset, TIER_5_BPS);
		}

		#[block]
		{
			// This depends on the number active boosts:
			assert_ok!(Pallet::<T, I>::stop_boosting(
				RawOrigin::Signed(lp_account).into(),
				asset,
				TIER_5_BPS
			));
		}

		assert_eq!(
			BoostPools::<T, I>::get(asset, TIER_5_BPS).unwrap().get_available_amount(),
			0u32.into()
		);
	}

	// This benchmark is currently not used (since we use the more computationally expensive
	// boost_finalised instead), but it is useful to keep around even if just to show that
	// boosting a deposit is relatively cheap.
	#[benchmark]
	fn deposit_boosted() {
		create_boost_pool::<T, I>();

		let asset = TargetChainAsset::<T, I>::iter().next().unwrap();

		const BOOSTER_COUNT: u32 = 100;

		let boosters: Vec<_> =
			(0..BOOSTER_COUNT).map(|i| setup_booster_account::<T, I>(asset, i)).collect();

		for booster_id in &boosters {
			assert_ok!(Pallet::<T, I>::add_boost_funds(
				RawOrigin::Signed(booster_id.clone()).into(),
				asset,
				1_000_000u32.into(),
				TIER_5_BPS
			));
		}

		let (_channel_id, deposit_address, ..) = Pallet::<T, I>::open_channel(
			&boosters[0],
			asset,
			ChannelAction::LiquidityProvision {
				lp_account: boosters[0].clone(),
				refund_address: None,
			},
			TIER_5_BPS,
		)
		.unwrap();

		let amount_before =
			BoostPools::<T, I>::get(asset, TIER_5_BPS).unwrap().get_available_amount();

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
			BoostPools::<T, I>::get(asset, TIER_5_BPS).unwrap().get_available_amount() <
				amount_before
		)
	}

	#[benchmark]
	fn boost_finalised() {
		use sp_runtime::Percent;
		use strum::IntoEnumIterator;

		create_boost_pool::<T, I>();

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
				TIER_5_BPS
			));
		}

		let deposit_address = prewitness_deposit::<T, I>(&boosters[0], asset, TIER_5_BPS);

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
				TIER_5_BPS
			));
		}

		let amount_before =
			BoostPools::<T, I>::get(asset, TIER_5_BPS).unwrap().get_available_amount();

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
			BoostPools::<T, I>::get(asset, TIER_5_BPS).unwrap().get_available_amount() >
				amount_before
		)
	}

	#[benchmark]
	fn create_boost_pools() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		let new_pools =
			vec![BoostPoolId { asset: BenchmarkValue::benchmark_value(), tier: TIER_5_BPS }];

		assert_eq!(BoostPools::<T, I>::iter().count(), 0);
		#[block]
		{
			assert_ok!(Pallet::<T, I>::create_boost_pools(origin, new_pools.clone()));
		}
		assert_eq!(BoostPools::<T, I>::iter().count(), 1);
	}

	#[benchmark]
	fn mark_transaction_as_tainted() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Broker).unwrap();
		// let tx_id = <<T as Config<I>>::TargetChain as Chain>::DepositDetails::benchmark_value();
		let tx_id: TransactionInIdFor<T, I> = TransactionInIdFor::<T, I>::benchmark_value();

		#[block]
		{
			assert_ok!(Pallet::<T, I>::mark_transaction_as_tainted_inner(
				caller.clone(),
				tx_id.clone(),
			));
		}

		assert!(
			TaintedTransactions::<T, I>::get(caller, tx_id).is_some(),
			"No tainted transactions found"
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
		new_test_ext().execute_with(|| {
			_add_boost_funds::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_process_deposit_as_lost::<Test, ()>(100, true);
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
		new_test_ext().execute_with(|| {
			_create_boost_pools::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_contract_swap_request::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_contract_ccm_swap_request::<Test, ()>(true);
		});
		new_test_ext().execute_with(|| {
			_mark_transaction_as_tainted::<Test, ()>(true);
		});
	}
}
