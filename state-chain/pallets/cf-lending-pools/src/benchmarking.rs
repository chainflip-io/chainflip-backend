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
	use crate::general_lending::{GeneralLoan, LiquidationStatus};
	use cf_chains::{btc::ScriptPubkey, evm::U256, ForeignChainAddress};
	use frame_support::sp_runtime::FixedU64;

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

	pub fn register_refund_addresses<T: Config>(account_id: &T::AccountId) {
		for encoded_address in [
			ForeignChainAddress::Eth(Default::default()),
			ForeignChainAddress::Dot(Default::default()),
			ForeignChainAddress::Btc(ScriptPubkey::Taproot([4u8; 32])),
			ForeignChainAddress::Sol(Default::default()),
		] {
			T::LpRegistrationApi::register_liquidity_refund_address(&account_id, encoded_address);
		}
	}

	fn setup_lp_account<T: Config>(asset: Asset, seed: u32) -> T::AccountId {
		use frame_support::traits::OnNewAccount;
		let caller: T::AccountId = account("lp", 0, seed);

		register_refund_addresses::<T>(&caller);

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

	fn gov_origin<T: Config>() -> <T as frame_system::Config>::RuntimeOrigin {
		T::EnsureGovernance::try_successful_origin().unwrap()
	}

	fn create_loan<T: Config>(borrower: &T::AccountId, amount: AssetAmount) -> GeneralLoan<T>
	where
		T: Config,
	{
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, amount * 2)]);
		assert_ok!(Pallet::<T>::request_loan(
			RawOrigin::Signed(borrower.clone()).into(),
			LOAN_ASSET,
			amount,
			Some(COLLATERAL_ASSET),
			collateral
		));
		let loan_account = LoanAccounts::<T>::get(borrower).unwrap();
		loan_account.loans.get(&LoanId::from(0)).unwrap().clone()
	}

	fn create_pools_and_loans_for_all_assets<T: Config>(
		borrower: &T::AccountId,
		lender: &T::AccountId,
	) {
		const AMOUNT: AssetAmount = 100_000_000;

		// Setup a loan account with some collateral and loans
		for asset in [Asset::Eth, Asset::Btc, Asset::Sol, Asset::Usdc, Asset::Usdt] {
			// Setup the pool with a lender
			assert_ok!(Pallet::<T>::create_lending_pool(gov_origin::<T>(), asset));
			set_asset_price_in_usd::<T>(asset, 100_000_000_000);
			T::Balance::credit_account(&lender, asset, AMOUNT * 2);
			assert_ok!(Pallet::<T>::add_lender_funds(
				RawOrigin::Signed(lender.clone()).into(),
				asset,
				AMOUNT * 2,
			));

			// Create the loan with collateral (same assets for simplicity)
			T::Balance::credit_account(borrower, asset, AMOUNT);
			assert_ok!(Pallet::<T>::request_loan(
				RawOrigin::Signed(borrower.clone()).into(),
				asset,
				AMOUNT / 2,
				Some(asset),
				BTreeMap::from([(asset, AMOUNT)]),
			));
		}
	}

	#[benchmark]
	fn update_pallet_config(n: Linear<1, MAX_PALLET_CONFIG_UPDATE>) {
		let origin = gov_origin::<T>();
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

		#[extrinsic_call]
		add_boost_funds(RawOrigin::Signed(lp_account.clone()), asset, amount, TIER_5_BPS);

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

		#[extrinsic_call]
		stop_boosting(RawOrigin::Signed(lp_account), asset, TIER_5_BPS);

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
		let origin = gov_origin::<T>();

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

		assert_ok!(Pallet::<T>::create_lending_pool(gov_origin::<T>(), LOAN_ASSET));

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
		let origin = gov_origin::<T>();

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

		#[extrinsic_call]
		add_lender_funds(RawOrigin::Signed(lender.clone()), LOAN_ASSET, AMOUNT);

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
		let origin = RawOrigin::Signed(lender.clone());
		assert_ok!(Pallet::<T>::add_lender_funds(origin.clone().into(), LOAN_ASSET, AMOUNT));
		let total_before = GeneralLendingPools::<T>::get(LOAN_ASSET).unwrap().total_amount;

		#[extrinsic_call]
		remove_lender_funds(origin, LOAN_ASSET, Some(AMOUNT));

		let pool = GeneralLendingPools::<T>::get(LOAN_ASSET).unwrap();
		assert_eq!(total_before - pool.total_amount, AMOUNT);
		assert!(!pool.lender_shares.contains_key(&lender));
	}

	#[benchmark]
	fn add_collateral() {
		const AMOUNT: AssetAmount = 100_000_000;
		set_asset_price_in_usd::<T>(COLLATERAL_ASSET, 200_000_000_000);
		let borrower = setup_lp_account::<T>(LOAN_ASSET, 0);
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, AMOUNT)]);

		#[extrinsic_call]
		add_collateral(RawOrigin::Signed(borrower), Some(COLLATERAL_ASSET), collateral.clone());

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
		let origin = RawOrigin::Signed(borrower.clone());
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, INITIAL_COLLATERAL)]);

		// Create a loan with collateral so it must perform checks when removing collateral
		assert_ok!(Pallet::<T>::request_loan(
			origin.clone().into(),
			LOAN_ASSET,
			LOAN_AMOUNT,
			Some(COLLATERAL_ASSET),
			collateral.clone(),
		));

		let loan_account = LoanAccounts::<T>::iter().next().unwrap().1;
		assert_eq!(loan_account.get_total_collateral(), collateral.clone());

		let collateral = BTreeMap::from([(COLLATERAL_ASSET, REMOVE_COLLATERAL)]);

		#[extrinsic_call]
		remove_collateral(origin, collateral);

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
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, 200_000_000)]);
		const LOAN_AMOUNT: AssetAmount = 50_000_000;

		#[extrinsic_call]
		request_loan(
			RawOrigin::Signed(borrower),
			LOAN_ASSET,
			LOAN_AMOUNT,
			Some(COLLATERAL_ASSET),
			collateral,
		);

		assert!(LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap() > 0);
	}

	#[benchmark]
	fn expand_loan() {
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);

		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let origin = RawOrigin::Signed(borrower.clone());
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, 200_000_000)]);
		const LOAN_AMOUNT: AssetAmount = 50_000_000;
		assert_ok!(Pallet::<T>::request_loan(
			origin.clone().into(),
			LOAN_ASSET,
			LOAN_AMOUNT,
			Some(COLLATERAL_ASSET),
			collateral
		));
		let value_before =
			LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap();

		#[extrinsic_call]
		expand_loan(origin, 0.into(), 5_000_000, BTreeMap::from([(COLLATERAL_ASSET, 100_000_000)]));

		assert!(
			LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap() >
				value_before
		);
	}

	#[benchmark]
	fn make_repayment() {
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);

		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let origin = RawOrigin::Signed(borrower.clone());
		let collateral = BTreeMap::from([(COLLATERAL_ASSET, 200_000_000)]);
		const LOAN_AMOUNT: AssetAmount = 50_000_000;
		assert_ok!(Pallet::<T>::request_loan(
			origin.clone().into(),
			LOAN_ASSET,
			LOAN_AMOUNT,
			Some(COLLATERAL_ASSET),
			collateral
		));
		let value_before =
			LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap();

		#[extrinsic_call]
		make_repayment(origin, 0.into(), RepaymentAmount::Exact(5_000_000));

		assert!(
			LoanAccounts::<T>::iter().next().unwrap().1.total_owed_usd_value().unwrap() <
				value_before
		);
	}

	#[benchmark]
	fn update_primary_collateral_asset() {
		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let origin = RawOrigin::Signed(borrower.clone());
		let collateral =
			BTreeMap::from([(COLLATERAL_ASSET, 200_000_000), (LOAN_ASSET, 100_000_000)]);

		set_asset_price_in_usd::<T>(LOAN_ASSET, 100_000_000_000);
		set_asset_price_in_usd::<T>(COLLATERAL_ASSET, 200_000_000_000);
		T::Balance::credit_account(&borrower, LOAN_ASSET, 100_000_000);
		assert_ok!(Pallet::<T>::add_collateral(
			origin.clone().into(),
			Some(COLLATERAL_ASSET),
			collateral.clone()
		));

		#[extrinsic_call]
		update_primary_collateral_asset(origin, LOAN_ASSET);

		assert_eq!(
			get_loan_accounts::<T>(Some(borrower)).first().unwrap().primary_collateral_asset,
			LOAN_ASSET
		);
	}

	#[benchmark]
	fn usd_value_of() {
		set_asset_price_in_usd::<T>(COLLATERAL_ASSET, 200_000_000_000);

		#[block]
		{
			assert_eq!(
				general_lending::usd_value_of::<T>(COLLATERAL_ASSET, 1_000_000).unwrap(),
				200_000_000_000_000_000_u128,
			);
		}
	}

	#[benchmark]
	fn initiate_network_fee_swap() {
		#[block]
		{
			general_lending::initiate_network_fee_swap::<T>(
				COLLATERAL_ASSET,
				1_000_000, // fee amount
			);
		}
	}

	#[benchmark]
	fn derive_ltv() {
		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let lender = setup_lp_account::<T>(LOAN_ASSET, 1);

		create_pools_and_loans_for_all_assets::<T>(&borrower, &lender);
		let loan_account = LoanAccounts::<T>::get(borrower).unwrap();

		#[block]
		{
			assert_ok!(loan_account.derive_ltv());
		}
	}

	#[benchmark]
	fn loan_charge_interest() {
		const AT_BLOCK: u32 = 100;
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);
		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let mut loan = create_loan::<T>(&borrower, 100_000_000);

		#[block]
		{
			assert_ok!(loan.charge_interest(
				FixedU64::from_rational(75, 100),
				AT_BLOCK.into(),
				AT_BLOCK - 1,
				&LendingConfig::<T>::get(),
			));
		}
		assert_eq!(loan.last_interest_payment_at, AT_BLOCK.into());
	}

	#[benchmark]
	fn loan_calculate_top_up_amount() {
		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let lender = setup_lp_account::<T>(LOAN_ASSET, 1);

		create_pools_and_loans_for_all_assets::<T>(&borrower, &lender);
		let loan_account = LoanAccounts::<T>::get(borrower.clone()).unwrap();

		#[block]
		{
			assert_ok!(loan_account
				.calculate_top_up_amount(&borrower, LENDING_DEFAULT_CONFIG.ltv_thresholds.target));
		}
	}

	#[benchmark]
	fn start_liquidation_swaps() {
		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let lender = setup_lp_account::<T>(LOAN_ASSET, 1);
		create_pools_and_loans_for_all_assets::<T>(&borrower, &lender);
		let mut loan_account = LoanAccounts::<T>::get(&borrower).unwrap();

		#[block]
		{
			let collateral = loan_account.prepare_collateral_for_liquidation().unwrap();
			loan_account.init_liquidation_swaps(&borrower, collateral, LiquidationType::Hard);
		}

		assert!(matches!(loan_account.liquidation_status, LiquidationStatus::Liquidating { .. }));
	}

	#[benchmark]
	fn abort_liquidation_swaps() {
		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		let lender = setup_lp_account::<T>(LOAN_ASSET, 1);
		create_pools_and_loans_for_all_assets::<T>(&borrower, &lender);
		let mut loan_account = LoanAccounts::<T>::get(&borrower).unwrap();

		// Start the liquidation swaps
		let collateral = loan_account.prepare_collateral_for_liquidation().unwrap();
		loan_account.init_liquidation_swaps(&borrower, collateral, LiquidationType::Hard);
		assert!(matches!(loan_account.liquidation_status, LiquidationStatus::Liquidating { .. }));

		#[block]
		{
			loan_account.abort_liquidation_swaps(LiquidationCompletionReason::FullySwapped);
		}
		assert!(matches!(loan_account.liquidation_status, LiquidationStatus::NoLiquidation));
	}

	#[benchmark]
	fn change_voluntary_liquidation() {
		setup_lending_pool::<T>(NUMBER_OF_LENDERS);
		let borrower = setup_lp_account::<T>(COLLATERAL_ASSET, 0);
		create_loan::<T>(&borrower, 100_000_000);
		assert_eq!(
			LoanAccounts::<T>::get(&borrower).unwrap().voluntary_liquidation_requested,
			false
		);

		#[extrinsic_call]
		initiate_voluntary_liquidation(RawOrigin::Signed(borrower.clone()));

		assert_eq!(LoanAccounts::<T>::get(borrower).unwrap().voluntary_liquidation_requested, true);
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
		new_test_ext().execute_with(|| {
			_usd_value_of::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_initiate_network_fee_swap::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_derive_ltv::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_loan_charge_interest::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_loan_calculate_top_up_amount::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_start_liquidation_swaps::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_abort_liquidation_swaps::<Test>(true);
		});
		new_test_ext().execute_with(|| {
			_change_voluntary_liquidation::<Test>(true);
		});
	}
}
