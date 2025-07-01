// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_primitives::Asset;
use frame_benchmarking::v2::*;
use frame_support::assert_ok;
use frame_system::RawOrigin;
use sp_std::vec;

// Keep this to avoid CI warnings about no benchmarks in the crate.
#[benchmarks]
mod benchmarks {
	use super::*;

	const TIER_5_BPS: BoostPoolTier = 5;

	fn create_boost_pool<T: Config>() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		assert_ok!(Pallet::<T>::create_boost_pools(
			origin,
			vec![BoostPoolId { asset: Asset::Eth, tier: TIER_5_BPS }]
		));
	}

	fn setup_chp_pool<T: Config>(asset: Asset) -> CorePoolId {
		let pool_id = NextCorePoolId::<T>::get();
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		assert_ok!(Pallet::<T>::create_chp_pool(origin, asset));
		pool_id
	}

	fn setup_lp_account<T: Config>(asset: Asset, seed: u32) -> T::AccountId {
		use frame_support::traits::OnNewAccount;
		let caller: T::AccountId = account("booster", 0, seed);

		// TODO: remove once https://github.com/chainflip-io/chainflip-backend/pull/4716 is merged
		if frame_system::Pallet::<T>::providers(&caller) == 0u32 {
			frame_system::Pallet::<T>::inc_providers(&caller);
		}
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller));
		T::Balance::credit_account(&caller, asset, 1_000_000);

		T::Balance::credit_account(&caller, asset, 5_000_000_000_000_000_000u128);
		T::Balance::credit_account(&caller, COLLATERAL_ASSET, 5_000_000_000_000_000_000u128);

		caller
	}

	fn chp_loan<T: Config>(
		asset: Asset,
		borrower: T::AccountId,
		core_pool_id: CorePoolId,
		id: u64,
		status: LoanStatus,
	) -> ChpLoan<T> {
		ChpLoan::<T>::new(
			ChpLoanId(id),
			asset,
			1u32.into(),
			1_000u32.into(),
			borrower,
			1_000_000_000_000_000u128,
			0u128,
			vec![ChpPoolContribution { core_pool_id, loan_id: LoanId(id), principal: 1_000u128 }],
			Perbill::from_parts(100_000),
			Default::default(),
			status,
		)
	}

	#[benchmark]
	fn update_pallet_config(n: Linear<1, MAX_PALLET_CONFIG_UPDATE>) {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let updates = vec![
			PalletConfigUpdate::SetNetworkFeeDeductionFromBoost {
				deduction_percent: Percent::from_percent(10),
			};
			n as usize
		]
		.try_into()
		.expect("Length is within the configured len");

		#[block]
		{
			assert_ok!(Pallet::<T>::update_pallet_config(origin, updates));
		}
	}

	#[benchmark]
	fn add_boost_funds() {
		create_boost_pool::<T>();

		let amount: AssetAmount = 1000;

		let asset = Asset::Eth;

		let lp_account = setup_lp_account::<T>(asset, 0);

		#[block]
		{
			assert_ok!(Pallet::<T>::add_boost_funds(
				RawOrigin::Signed(lp_account.clone()).into(),
				asset,
				amount,
				TIER_5_BPS
			));
		}

		let boost_pool = BoostPools::<T>::get(asset, TIER_5_BPS).unwrap();

		assert_eq!(
			CorePools::<T>::get(asset, boost_pool.core_pool_id)
				.unwrap()
				.get_available_amount(),
			amount
		);
	}

	#[benchmark]
	fn process_deposit_as_lost(n: Linear<1, 100>) {
		create_boost_pool::<T>();

		const ASSET: Asset = Asset::Eth;
		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(0);

		let boosters: Vec<_> = (0..n).map(|i| setup_lp_account::<T>(ASSET, i)).collect();

		for booster_id in &boosters {
			assert_ok!(Pallet::<T>::add_boost_funds(
				RawOrigin::Signed(booster_id.clone()).into(),
				ASSET,
				1_000_000u32.into(),
				TIER_5_BPS
			));
		}

		assert_ok!(Pallet::<T>::try_boosting(DEPOSIT_ID, ASSET, 1000, TIER_5_BPS));

		// Worst-case scenario is when all boosters withdraw funds while
		// waiting for the deposit to be finalised:
		for booster_id in &boosters {
			assert_ok!(Pallet::<T>::stop_boosting(
				RawOrigin::Signed(booster_id.clone()).into(),
				ASSET,
				TIER_5_BPS
			));
		}

		#[block]
		{
			Pallet::<T>::process_deposit_as_lost(DEPOSIT_ID, ASSET);
		}
	}

	#[benchmark]
	fn stop_boosting() {
		create_boost_pool::<T>();

		let asset = Asset::Eth;

		let lp_account = setup_lp_account::<T>(asset, 0);

		assert_ok!(Pallet::<T>::add_boost_funds(
			RawOrigin::Signed(lp_account.clone()).into(),
			asset,
			1_000_000u32.into(),
			TIER_5_BPS
		));

		// `stop_boosting` has linear complexity w.r.t. the number of pending boosts,
		// and this seems like a reasonable estimate:
		const PENDING_BOOSTS_COUNT: usize = 50;

		for deposit_id in 0..PENDING_BOOSTS_COUNT {
			assert_ok!(Pallet::<T>::try_boosting(
				PrewitnessedDepositId(deposit_id as u64),
				asset,
				1000,
				TIER_5_BPS
			));
		}

		#[block]
		{
			// This depends on the number active boosts:
			assert_ok!(Pallet::<T>::stop_boosting(
				RawOrigin::Signed(lp_account).into(),
				asset,
				TIER_5_BPS
			));
		}

		let boost_pool = BoostPools::<T>::get(asset, TIER_5_BPS).unwrap();

		assert_eq!(
			CorePools::<T>::get(asset, boost_pool.core_pool_id)
				.unwrap()
				.get_available_amount(),
			0
		);
	}

	#[benchmark]
	fn create_boost_pools() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		let new_pools = vec![BoostPoolId { asset: Asset::Eth, tier: TIER_5_BPS }];

		assert_eq!(BoostPools::<T>::iter().count(), 0);
		#[block]
		{
			assert_ok!(Pallet::<T>::create_boost_pools(origin, new_pools.clone()));
		}
		assert_eq!(BoostPools::<T>::iter().count(), 1);
	}

	#[benchmark]
	fn create_chp_pool() {
		#[block]
		{
			setup_chp_pool::<T>(Asset::Btc);
		}
	}

	#[benchmark]
	fn add_chp_funds() {
		let asset = Asset::Btc;
		let lp_account = setup_lp_account::<T>(asset, 0);
		setup_chp_pool::<T>(asset);

		#[block]
		{
			assert_ok!(Pallet::<T>::add_chp_funds(
				RawOrigin::Signed(lp_account).into(),
				asset,
				1_000_000_000_000u128,
			));
		}
	}

	#[benchmark]
	fn stop_chp_lending() {
		let asset = Asset::Btc;
		let lp_account = setup_lp_account::<T>(asset, 0);
		let core_pool_id = NextCorePoolId::<T>::get();

		setup_chp_pool::<T>(asset);
		assert_ok!(Pallet::<T>::add_chp_funds(
			RawOrigin::Signed(lp_account.clone()).into(),
			asset,
			1_000_000_000_000u128,
		));

		// Pessimistically add pending loans.
		CorePools::<T>::mutate(asset, core_pool_id, |maybe_pool| {
			let pool = maybe_pool.as_mut().expect("Pool was created above");
			for i in 0..10 {
				assert_ok!(pool.new_loan(1_000_000_000u128, LoanUsage::ChpLoan(ChpLoanId(i))));
			}
		});

		#[block]
		{
			assert_ok!(Pallet::<T>::stop_chp_lending(RawOrigin::Signed(lp_account).into(), asset,));
		}
	}

	#[benchmark]
	fn upkeep_active(n: Linear<1, 50>) {
		let asset = Asset::Btc;

		let lp_account = setup_lp_account::<T>(asset, 0);
		let borrower = setup_lp_account::<T>(Asset::Usdc, 1);
		let core_pool_id = setup_chp_pool::<T>(asset);

		assert_ok!(Pallet::<T>::add_chp_funds(
			RawOrigin::Signed(lp_account).into(),
			asset,
			1_000_000_000_000u128,
		));

		for i in 0..n {
			ChpLoans::<T>::insert(
				asset,
				ChpLoanId(i as u64),
				chp_loan::<T>(asset, borrower.clone(), core_pool_id, i as u64, LoanStatus::Active),
			);
		}

		let initial_collateral = ChpLoans::<T>::get(asset, ChpLoanId(0)).unwrap().usdc_collateral();
		let initial_balance = T::Balance::get_balance(&borrower, COLLATERAL_ASSET);

		#[block]
		{
			crate::chp_lending::chp_upkeep::<T>(2u32.into());
		}

		for i in 0..n {
			let loan = ChpLoans::<T>::get(asset, ChpLoanId(i as u64)).unwrap();

			// No interest is charged
			assert!(loan.fees_collected_usdc().is_zero());

			// No topup is done
			assert_eq!(initial_balance, T::Balance::get_balance(&borrower, COLLATERAL_ASSET));

			// No liquidation is initiated
			assert_eq!(initial_collateral, loan.usdc_collateral());
		}
	}

	#[benchmark]
	fn upkeep_soft_liquidation(n: Linear<1, 50>) {
		let asset = Asset::Btc;

		let lp_account = setup_lp_account::<T>(asset, 0);
		let borrower = setup_lp_account::<T>(Asset::Usdc, 1);
		let core_pool_id = setup_chp_pool::<T>(asset);

		assert_ok!(Pallet::<T>::add_chp_funds(
			RawOrigin::Signed(lp_account).into(),
			asset,
			1_000_000_000_000u128,
		));

		for i in 0..n {
			ChpLoans::<T>::insert(
				asset,
				ChpLoanId(i as u64),
				chp_loan::<T>(
					asset,
					borrower.clone(),
					core_pool_id,
					i as u64,
					LoanStatus::SoftLiquidation { usdc_collateral: 1_000_000u128 },
				),
			);
		}

		#[block]
		{
			crate::chp_lending::chp_upkeep::<T>(2u32.into());
		}
	}

	#[benchmark]
	fn upkeep_no_action(n: Linear<1, 50>) {
		let asset = Asset::Btc;

		let lp_account = setup_lp_account::<T>(asset, 0);
		let borrower = setup_lp_account::<T>(Asset::Usdc, 1);
		let core_pool_id = setup_chp_pool::<T>(asset);

		assert_ok!(Pallet::<T>::add_chp_funds(
			RawOrigin::Signed(lp_account).into(),
			asset,
			1_000_000_000_000u128,
		));

		for i in 0..n {
			ChpLoans::<T>::insert(
				asset,
				ChpLoanId(i as u64),
				chp_loan::<T>(
					asset,
					borrower.clone(),
					core_pool_id,
					i as u64,
					LoanStatus::Finalising,
				),
			);
		}

		#[block]
		{
			crate::chp_lending::chp_upkeep::<T>(2u32.into());
		}
	}

	#[benchmark]
	fn charge_interest_for_loan(n: Linear<1, 20>) {
		let asset = Asset::Btc;
		let lp_account = setup_lp_account::<T>(asset, 0);
		let core_pool_id = setup_chp_pool::<T>(asset);
		let initial_collateral = 1_000_000_000_000u128;
		let mut loan = ChpLoan::<T>::new(
			ChpLoanId(1),
			asset,
			Default::default(),
			(INTEREST_PAYMENT_INTERVAL * 10).into(),
			lp_account,
			initial_collateral,
			0u128,
			(0..n)
				.map(|id| ChpPoolContribution {
					core_pool_id,
					loan_id: LoanId(id as u64),
					principal: 1_000_000u128,
				})
				.collect::<Vec<_>>(),
			Perbill::from_parts(1_000_000),
			cf_amm_math::price_at_tick(0).unwrap(),
			LoanStatus::Active,
		);

		#[block]
		{
			let _ = crate::chp_lending::process_interest_for_loan::<T>(
				INTEREST_PAYMENT_INTERVAL.into(),
				&mut loan,
			);
		}

		assert!(loan.usdc_collateral() < initial_collateral);
		assert!(loan.fees_collected_usdc() > 0);
	}

	#[benchmark]
	fn top_up_collateral() {
		let asset = Asset::Btc;
		let lp_account = setup_lp_account::<T>(asset, 0);
		let core_pool_id = setup_chp_pool::<T>(asset);
		let config = ChpConfiguration {
			overcollateralisation_topup_threshold: Permill::from_percent(50),
			overcollateralisation_target: Permill::one(),
			..Default::default()
		};

		ChpConfig::<T>::set(config.clone());

		let mut loan = ChpLoan::<T>::new(
			ChpLoanId(1),
			asset,
			Default::default(),
			(INTEREST_PAYMENT_INTERVAL * 10).into(),
			lp_account.clone(),
			1_000_000u128,
			0u128,
			vec![ChpPoolContribution {
				core_pool_id,
				loan_id: LoanId(1u64),
				principal: 1_000_000u128,
			}],
			Perbill::from_parts(1_000_000),
			cf_amm_math::price_at_tick(0).unwrap(),
			LoanStatus::Active,
		);

		let initial_balance = T::Balance::get_balance(&lp_account, COLLATERAL_ASSET);

		#[block]
		{
			let _ = crate::chp_lending::process_collateral_topup::<T>(&mut loan, &config);
		}

		// Ensure balance is reduced due to collateral top ups
		assert!(T::Balance::get_balance(&lp_account, COLLATERAL_ASSET) < initial_balance);
	}

	#[benchmark]
	fn initiate_soft_liquidation() {
		let asset = Asset::Btc;
		let lp_account = setup_lp_account::<T>(asset, 0);
		let core_pool_id = setup_chp_pool::<T>(asset);

		let mut loan = ChpLoan::<T>::new(
			ChpLoanId(1),
			asset,
			Default::default(),
			(INTEREST_PAYMENT_INTERVAL * 10).into(),
			lp_account.clone(),
			1_000_000u128,
			0u128,
			vec![ChpPoolContribution {
				core_pool_id,
				loan_id: LoanId(1u64),
				principal: 1_000_000u128,
			}],
			Perbill::from_parts(1_000_000),
			cf_amm_math::price_at_tick(0).unwrap(),
			LoanStatus::Active,
		);

		#[block]
		{
			crate::chp_lending::initiate_soft_liquidation::<T>(&mut loan);
		}

		// All collaterals are liquidated
		assert!(loan.usdc_collateral().is_zero());
	}

	impl_benchmark_test_suite!(Pallet, crate::mocks::new_test_ext(), crate::mocks::Test,);
}
