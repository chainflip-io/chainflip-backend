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

#[benchmarks]
mod benchmarks {
	use super::*;
	use cf_chains::evm::U256;

	const TIER_5_BPS: BoostPoolTier = 5;
	const COLLATERAL_ASSET: Asset = Asset::Eth;
	const LOAN_ASSET: Asset = Asset::Btc;
	const NUMBER_OF_LENDERS: u32 = 1000;

	fn set_asset_price_in_usd<T: Config>(asset: Asset, price: u128) {
		const PRICE_FRACTIONAL_BITS: u32 = 128;
		<T as Config>::PriceApi::set_price(asset, U256::from(price) << PRICE_FRACTIONAL_BITS);
	}

	fn create_boost_pool<T: Config>() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		assert_ok!(Pallet::<T>::create_boost_pools(
			origin,
			vec![BoostPoolId { asset: Asset::Eth, tier: TIER_5_BPS }]
		));
	}

	fn setup_lp_account<T: Config>(asset: Asset, seed: u32) -> T::AccountId {
		use frame_support::traits::OnNewAccount;
		let caller: T::AccountId = account("lp", 0, seed);

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

	// Creates a lending pool for the loan asset and adds a bunch of lenders, leaving seed 0 free
	// for `setup_lp_account`. Also sets a price for the loan and collateral assets.
	fn setup_lending_pool<T: Config>(number_of_lenders: u32) {
		set_asset_price_in_usd::<T>(LOAN_ASSET, 100_000_000_000);
		set_asset_price_in_usd::<T>(COLLATERAL_ASSET, 200_000_000_000);

		let gov_origin = T::EnsureGovernance::try_successful_origin().unwrap();
		assert_ok!(Pallet::<T>::create_lending_pool(gov_origin, LOAN_ASSET));

		for i in 1..=number_of_lenders {
			let lender = setup_lp_account::<T>(LOAN_ASSET, i);
			assert_ok!(Pallet::<T>::add_lender_funds(
				RawOrigin::Signed(lender.clone()).into(),
				LOAN_ASSET,
				(i * 1_000_000) as u128,
			));
		}
	}

	#[benchmark]
	fn create_lending_pool() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		assert_eq!(GeneralLendingPools::<T>::iter().count(), 0);
		#[block]
		{
			assert_ok!(Pallet::<T>::create_lending_pool(origin, LOAN_ASSET));
		}
		assert_eq!(GeneralLendingPools::<T>::iter().count(), 1);
	}

	#[benchmark]
	fn add_lender_funds() {
		const AMOUNT: AssetAmount = 100_000_000;

		// Setup a lending pool with lots of lenders, so it has to recalculate the shares.
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);
		let total_before = GeneralLendingPools::<T>::get(LOAN_ASSET).unwrap().total_amount;

		// Create one more lender and do the add lender funds operation
		let lender = setup_lp_account::<T>(LOAN_ASSET, 0);
		let origin = RawOrigin::Signed(lender.clone()).into();
		#[block]
		{
			assert_ok!(Pallet::<T>::add_lender_funds(origin, LOAN_ASSET, AMOUNT));
		}

		let pool = GeneralLendingPools::<T>::get(LOAN_ASSET).unwrap();
		assert_eq!(pool.total_amount - total_before, AMOUNT);
	}

	#[benchmark]
	fn remove_lender_funds() {
		const AMOUNT: AssetAmount = 100_000_000;

		// Setup a lending pool with lots of lenders, so it has to recalculate the shares.
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);

		// Create a lender account and add funds to be removed
		let lender = setup_lp_account::<T>(LOAN_ASSET, 0);
		let origin: OriginFor<T> = RawOrigin::Signed(lender.clone()).into();
		assert_ok!(Pallet::<T>::add_lender_funds(origin.clone(), LOAN_ASSET, AMOUNT));
		let total_before = GeneralLendingPools::<T>::get(LOAN_ASSET).unwrap().total_amount;

		#[block]
		{
			assert_ok!(Pallet::<T>::remove_lender_funds(origin, LOAN_ASSET, Some(AMOUNT)));
		}

		let pool = GeneralLendingPools::<T>::get(LOAN_ASSET).unwrap();
		assert_eq!(total_before - pool.total_amount, AMOUNT);
		assert!(!pool.lender_shares.contains_key(&lender));
	}

	#[benchmark]
	fn add_collateral() {
		const AMOUNT: AssetAmount = 100_000_000;
		set_asset_price_in_usd::<T>(COLLATERAL_ASSET, 200_000_000_000);
		let lender = setup_lp_account::<T>(LOAN_ASSET, 0);
		let origin: OriginFor<T> = RawOrigin::Signed(lender.clone()).into();
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, AMOUNT)]);

		#[block]
		{
			assert_ok!(Pallet::<T>::add_collateral(
				origin,
				Some(COLLATERAL_ASSET),
				collateral.clone()
			));
		}
		let loan_account = LoanAccounts::<T>::iter().next().unwrap().1;
		assert_eq!(loan_account.get_total_collateral(), collateral);
	}

	#[benchmark]
	fn remove_collateral() {
		const INITIAL_COLLATERAL: AssetAmount = 100_000_000;
		const REMOVE_COLLATERAL: AssetAmount = 10_000_000;
		const LOAN_AMOUNT: AssetAmount = 50_000_000;
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);
		let borrower = setup_lp_account::<T>(LOAN_ASSET, 0);
		let origin: OriginFor<T> = RawOrigin::Signed(borrower.clone()).into();
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, INITIAL_COLLATERAL)]);

		// Create a loan with collateral so it must perform checks when removing collateral
		assert_ok!(Pallet::<T>::request_loan(
			origin.clone(),
			LOAN_ASSET,
			LOAN_AMOUNT,
			Some(COLLATERAL_ASSET),
			collateral.clone(),
		));

		let loan_account = LoanAccounts::<T>::iter().next().unwrap().1;
		assert_eq!(loan_account.get_total_collateral(), collateral.clone());

		let collateral = BTreeMap::from([(COLLATERAL_ASSET, REMOVE_COLLATERAL)]);
		#[block]
		{
			assert_ok!(Pallet::<T>::remove_collateral(origin, collateral));
		}
		assert_eq!(
			get_loan_accounts::<T>(Some(borrower))
				.first()
				.unwrap()
				.collateral
				.first()
				.unwrap()
				.amount,
			INITIAL_COLLATERAL - REMOVE_COLLATERAL,
		);
	}

	#[benchmark]
	fn request_loan() {
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);

		T::PriceApi::get_price(LOAN_ASSET).unwrap();

		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let origin: OriginFor<T> = RawOrigin::Signed(borrower.clone()).into();
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, 200_000_000)]);
		const LOAN_AMOUNT: AssetAmount = 50_000_000;

		#[block]
		{
			assert_ok!(Pallet::<T>::request_loan(
				origin,
				LOAN_ASSET,
				LOAN_AMOUNT,
				Some(COLLATERAL_ASSET),
				collateral
			));
		}
		assert!(LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap() > 0);
	}

	#[benchmark]
	fn expand_loan() {
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);

		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let origin: OriginFor<T> = RawOrigin::Signed(borrower.clone()).into();
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, 200_000_000)]);
		const LOAN_AMOUNT: AssetAmount = 50_000_000;
		assert_ok!(Pallet::<T>::request_loan(
			origin.clone(),
			LOAN_ASSET,
			LOAN_AMOUNT,
			Some(COLLATERAL_ASSET),
			collateral
		));
		let value_before =
			LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap();

		#[block]
		{
			assert_ok!(Pallet::<T>::expand_loan(
				origin,
				0.into(),
				5_000_000,
				BTreeMap::from([(COLLATERAL_ASSET, 100_000_000)])
			));
		}
		assert!(
			LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap() >
				value_before
		);
	}

	#[benchmark]
	fn make_repayment() {
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);

		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let origin: OriginFor<T> = RawOrigin::Signed(borrower.clone()).into();
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, 200_000_000)]);
		const LOAN_AMOUNT: AssetAmount = 50_000_000;
		assert_ok!(Pallet::<T>::request_loan(
			origin.clone(),
			LOAN_ASSET,
			LOAN_AMOUNT,
			Some(COLLATERAL_ASSET),
			collateral
		));
		let value_before =
			LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap();

		#[block]
		{
			assert_ok!(Pallet::<T>::make_repayment(origin, 0.into(), 5_000_000,));
		}
		assert!(
			LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap() <
				value_before
		);
	}

	#[benchmark]
	fn update_primary_collateral_asset() {
		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let origin: OriginFor<T> = RawOrigin::Signed(borrower.clone()).into();
		let collateral =
			BTreeMap::from([(COLLATERAL_ASSET, 200_000_000), (LOAN_ASSET, 100_000_000)]);

		set_asset_price_in_usd::<T>(LOAN_ASSET, 100_000_000_000);
		set_asset_price_in_usd::<T>(COLLATERAL_ASSET, 200_000_000_000);
		T::Balance::credit_account(&borrower, LOAN_ASSET, 100_000_000);
		assert_ok!(Pallet::<T>::add_collateral(
			origin.clone(),
			Some(COLLATERAL_ASSET),
			collateral.clone()
		));

		#[block]
		{
			assert_ok!(Pallet::<T>::update_primary_collateral_asset(origin, LOAN_ASSET,));
		}

		assert_eq!(
			get_loan_accounts::<T>(Some(borrower)).first().unwrap().primary_collateral_asset,
			LOAN_ASSET
		);
	}

	#[cfg(test)]
	use crate::mocks::{new_test_ext, Test};

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_create_lending_pool::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_add_lender_funds::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_remove_lender_funds::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_add_collateral::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_add_collateral::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_remove_collateral::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_request_loan::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_expand_loan::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_make_repayment::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_update_primary_collateral_asset::<Test>(true);
		});
	}
}
