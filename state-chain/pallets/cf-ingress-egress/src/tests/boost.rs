use super::*;

use cf_chains::FeeEstimationApi;
use cf_primitives::{AssetAmount, BasisPoints};
use cf_traits::{
	mocks::{
		account_role_registry::MockAccountRoleRegistry, tracked_data_provider::TrackedDataProvider,
	},
	AccountRoleRegistry, LpBalanceApi,
};

use crate::{BoostId, BoostPoolTier, BoostPools, DepositTracker, Event};

type AccountId = u64;
type DepositBalances = crate::DepositBalances<Test, ()>;

const LP_ACCOUNT: AccountId = 100;
const BOOSTER_1: AccountId = 101;
const BOOSTER_2: AccountId = 102;

const INIT_BOOSTER_ETH_BALANCE: AssetAmount = 1_000_000_000;
const INIT_BOOSTER_FLIP_BALANCE: AssetAmount = 1_000_000_000;
const INIT_LP_BALANCE: AssetAmount = 0;

// All fetched deposits represent two booster's initial balances:
const INIT_FETCHED_DEPOSITS: AssetAmount = 2 * INIT_BOOSTER_ETH_BALANCE;

// Amounts as computed by `setup`:
const INGRESS_FEE: AssetAmount = 1_000_000;

fn get_lp_balance(lp: &AccountId, asset: eth::Asset) -> AssetAmount {
	// let asset: Asset = ;
	let balances = <Test as crate::Config>::LpBalance::asset_balances(lp).unwrap();

	balances[asset.into()]
}

fn get_lp_eth_balance(lp: &AccountId) -> AssetAmount {
	get_lp_balance(lp, eth::Asset::Eth)
}

fn request_deposit_address(
	account_id: u64,
	asset: eth::Asset,
	max_boost_fee: BasisPoints,
) -> (u64, H160) {
	let (channel_id, deposit_address, ..) =
		IngressEgress::request_liquidity_deposit_address(account_id, asset, max_boost_fee).unwrap();

	(channel_id, deposit_address.try_into().unwrap())
}

fn request_deposit_address_eth(account_id: u64, max_boost_fee: BasisPoints) -> (u64, H160) {
	request_deposit_address(account_id, eth::Asset::Eth, max_boost_fee)
}

#[track_caller]
fn prewitness_deposit(deposit_address: H160, asset: eth::Asset, amount: AssetAmount) -> u64 {
	assert_ok!(Pallet::<Test, _>::add_prewitnessed_deposits(
		vec![DepositWitness::<Ethereum> { deposit_address, asset, amount, deposit_details: () }],
		0
	),);

	PrewitnessedDepositIdCounter::<Test, _>::get()
}

#[track_caller]
fn witness_deposit(deposit_address: H160, asset: eth::Asset, amount: AssetAmount) {
	assert_ok!(Pallet::<Test, _>::process_deposit_witnesses(
		vec![DepositWitness::<Ethereum> { deposit_address, asset, amount, deposit_details: () }],
		Default::default()
	));
}

fn get_available_amount(asset: eth::Asset, fee_tier: BoostPoolTier) -> AssetAmount {
	BoostPools::<Test>::get(asset, fee_tier).unwrap().get_available_amount()
}

// Setup accounts and ensure that ingress fee is `INGRESS_FEE`
fn setup() {
	assert_ok!(
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&LP_ACCOUNT,
		)
	);
	assert_ok!(
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&BOOSTER_1,
		)
	);
	assert_ok!(
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&BOOSTER_2,
		)
	);

	const TOTAL_DEPOSITS: AssetAmount = 2 * INIT_BOOSTER_ETH_BALANCE;

	DepositBalances::mutate(eth::Asset::Eth, |deposits| {
		deposits.register_deposit(TOTAL_DEPOSITS);
		deposits.mark_as_fetched(TOTAL_DEPOSITS);
	});

	assert_eq!(
		DepositBalances::get(eth::Asset::Eth),
		DepositTracker { fetched: TOTAL_DEPOSITS, unfetched: 0 }
	);

	for asset in eth::Asset::all() {
		assert_ok!(<Test as crate::Config>::LpBalance::try_credit_account(
			&BOOSTER_1,
			asset.into(),
			INIT_BOOSTER_ETH_BALANCE,
		));

		assert_ok!(<Test as crate::Config>::LpBalance::try_credit_account(
			&BOOSTER_2,
			asset.into(),
			INIT_BOOSTER_ETH_BALANCE,
		));
	}

	assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE);
	assert_eq!(get_lp_eth_balance(&BOOSTER_2), INIT_BOOSTER_ETH_BALANCE);
	assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), INIT_LP_BALANCE);

	let tracked_data = cf_chains::eth::EthereumTrackedData { base_fee: 10, priority_fee: 10 };

	ChainTracker::<Ethereum>::set_fee(INGRESS_FEE);

	TrackedDataProvider::<Ethereum>::set_tracked_data(tracked_data);
	assert_eq!(tracked_data.estimate_ingress_fee(eth::Asset::Eth), INGRESS_FEE);
}

#[test]
fn basic_passive_boosting() {
	new_test_ext().execute_with(|| {
		const ASSET: eth::Asset = eth::Asset::Eth;
		const DEPOSIT_AMOUNT: AssetAmount = 500_000_000;

		const BOOSTER_AMOUNT_1: AssetAmount = 250_000_000;
		const BOOSTER_AMOUNT_2: AssetAmount = 500_000_000;

		setup();

		// ==== Boosters add make funds available for boosting ====
		{
			assert_ok!(IngressEgress::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				ASSET,
				BOOSTER_AMOUNT_1,
				BoostPoolTier::FiveBps
			));

			assert_ok!(IngressEgress::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_2),
				ASSET,
				BOOSTER_AMOUNT_2,
				BoostPoolTier::TenBps
			));

			assert_eq!(get_available_amount(ASSET, BoostPoolTier::FiveBps), BOOSTER_AMOUNT_1);
			assert_eq!(get_available_amount(ASSET, BoostPoolTier::TenBps), BOOSTER_AMOUNT_2);

			assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE - BOOSTER_AMOUNT_1);
			assert_eq!(get_lp_eth_balance(&BOOSTER_2), INIT_BOOSTER_ETH_BALANCE - BOOSTER_AMOUNT_2);
		}

		// ==== LP sends funds to liquidity deposit address, which gets pre-witnessed ====
		assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), INIT_LP_BALANCE);
		let (channel_id, deposit_address) = request_deposit_address_eth(LP_ACCOUNT, 30);
		let deposit_id = prewitness_deposit(deposit_address, ASSET, DEPOSIT_AMOUNT);
		// All of BOOSTER_AMOUNT_1 should be used:
		const POOL_1_FEE: AssetAmount = BOOSTER_AMOUNT_1 * BoostPoolTier::FiveBps as u128 / 10_000;
		// Only part of BOOSTER_AMOUNT_2 should be used:
		const POOL_2_CONTRIBUTION: AssetAmount = DEPOSIT_AMOUNT - (BOOSTER_AMOUNT_1 + POOL_1_FEE);
		const POOL_2_FEE: AssetAmount =
			POOL_2_CONTRIBUTION * BoostPoolTier::TenBps as u128 / 10_000;
		const LP_BALANCE_AFTER_BOOST: AssetAmount =
			INIT_LP_BALANCE + DEPOSIT_AMOUNT - POOL_1_FEE - POOL_2_FEE - INGRESS_FEE;
		{
			System::assert_last_event(RuntimeEvent::IngressEgress(Event::DepositBoosted {
				deposit_address,
				asset: ASSET,
				amount: DEPOSIT_AMOUNT,
				deposit_details: (),
				ingress_fee: INGRESS_FEE,
				boost_fee: POOL_1_FEE + POOL_2_FEE,
				action: DepositAction::LiquidityProvision { lp_account: LP_ACCOUNT },
			}));

			assert_boosted(
				deposit_address,
				deposit_id,
				[BoostPoolTier::FiveBps, BoostPoolTier::TenBps],
			);

			const POOL_1_CONTRIBUTION: AssetAmount = BOOSTER_AMOUNT_1 + POOL_1_FEE;
			const POOL_2_CONTRIBUTION: AssetAmount = DEPOSIT_AMOUNT - POOL_1_CONTRIBUTION;

			// Channel action is immediately executed (LP gets credited in this case):
			assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), LP_BALANCE_AFTER_BOOST);

			// Deposit isn't fully witnessed yet, so there is no change to fetched balance
			// apart from part of it being reserved as ingress fee:
			assert_eq!(
				DepositBalances::get(ASSET),
				DepositTracker { fetched: INIT_FETCHED_DEPOSITS - INGRESS_FEE, unfetched: 0 }
			);

			assert_eq!(get_available_amount(ASSET, BoostPoolTier::FiveBps), 0);

			assert_eq!(
				get_available_amount(ASSET, BoostPoolTier::TenBps),
				BOOSTER_AMOUNT_2 - POOL_2_CONTRIBUTION + POOL_2_FEE
			);
		}

		// ======== Deposit is fully witnessed ========
		{
			witness_deposit(deposit_address, ASSET, DEPOSIT_AMOUNT);

			System::assert_last_event(RuntimeEvent::IngressEgress(Event::DepositFinalised {
				deposit_address,
				asset: ASSET,
				amount: DEPOSIT_AMOUNT,
				deposit_details: (),
				ingress_fee: 0,
				action: DepositAction::BoostersCredited,
			}));

			assert_eq!(PrewitnessedDeposits::<Test>::get(channel_id, deposit_id), None);

			assert_eq!(
				get_available_amount(ASSET, BoostPoolTier::FiveBps),
				BOOSTER_AMOUNT_1 + POOL_1_FEE
			);

			assert_eq!(
				get_available_amount(ASSET, BoostPoolTier::TenBps),
				BOOSTER_AMOUNT_2 + POOL_2_FEE
			);

			// Channel action should *not* be performed again (since it's been done at the time of
			// boosting), meaning LP's funds are unchanged:
			assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), LP_BALANCE_AFTER_BOOST);

			// The new deposit should now be reflected in the unfetched balance:
			assert_eq!(
				DepositBalances::get(ASSET),
				DepositTracker {
					fetched: INIT_FETCHED_DEPOSITS - INGRESS_FEE,
					unfetched: DEPOSIT_AMOUNT
				}
			);
		}
	});
}

#[test]
fn can_boost_non_eth_asset() {
	// All other tests assume Eth as the asset. Here we check
	// that the assumption didn't leak anywhere into non-test
	// code, showing that other assets can be boosted without
	// unexpectedly affecting Eth.

	for asset in eth::Asset::all() {
		if asset != eth::Asset::Eth {
			test_for_asset(asset);
		}
	}

	#[track_caller]
	fn test_for_asset(asset: eth::Asset) {
		new_test_ext().execute_with(|| {
			assert_ne!(asset, eth::Asset::Eth);

			const BOOSTER_AMOUNT_1: AssetAmount = 500_000_000;
			const DEPOSIT_AMOUNT: AssetAmount = 200_000_000;

			const BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT / 1000;

			setup();

			assert_ok!(IngressEgress::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				asset,
				BOOSTER_AMOUNT_1,
				BoostPoolTier::TenBps
			));

			let (_channel_id, deposit_address) = request_deposit_address(LP_ACCOUNT, asset, 30);

			assert_eq!(get_lp_balance(&LP_ACCOUNT, asset), 0);
			assert_eq!(get_lp_balance(&LP_ACCOUNT, eth::Asset::Eth), 0);

			// Set the prices to ensure that ingress fee does not default to 0
			{
				use mocks::asset_converter::MockAssetConverter;
				MockAssetConverter::set_price(Asset::Eth, asset.into(), 1u32.into());
				MockAssetConverter::set_price(asset.into(), Asset::Eth, 1u32.into());
			}

			// After prewitnessing, the deposit is boosted and LP is credited

			const LP_AMOUNT_AFTER_BOOST: AssetAmount = DEPOSIT_AMOUNT - BOOST_FEE - INGRESS_FEE;
			{
				prewitness_deposit(deposit_address, asset, DEPOSIT_AMOUNT);

				assert_eq!(
					get_lp_balance(&BOOSTER_1, asset),
					INIT_BOOSTER_FLIP_BALANCE - BOOSTER_AMOUNT_1
				);

				assert_eq!(
					get_available_amount(asset, BoostPoolTier::TenBps),
					BOOSTER_AMOUNT_1 - DEPOSIT_AMOUNT + BOOST_FEE
				);

				assert_eq!(get_lp_balance(&LP_ACCOUNT, asset), LP_AMOUNT_AFTER_BOOST);
				assert_eq!(get_lp_balance(&LP_ACCOUNT, eth::Asset::Eth), 0);
			}

			// After deposit is finalised, it is credited to the correct boost pool:
			{
				witness_deposit(deposit_address, asset, DEPOSIT_AMOUNT);
				assert_eq!(get_lp_balance(&LP_ACCOUNT, asset), LP_AMOUNT_AFTER_BOOST);
				assert_eq!(get_lp_balance(&LP_ACCOUNT, eth::Asset::Eth), 0);

				assert_eq!(
					get_available_amount(asset, BoostPoolTier::TenBps),
					BOOSTER_AMOUNT_1 + BOOST_FEE
				);

				assert_eq!(get_available_amount(eth::Asset::Eth, BoostPoolTier::TenBps), 0);
			}

			// Booster stops boosting and receives funds in the correct asset:
			{
				assert_ok!(IngressEgress::stop_boosting(
					RuntimeOrigin::signed(BOOSTER_1),
					asset,
					BoostPoolTier::TenBps
				));
				assert_eq!(
					get_lp_balance(&BOOSTER_1, asset),
					INIT_BOOSTER_FLIP_BALANCE + BOOST_FEE
				);
				assert_eq!(get_lp_balance(&BOOSTER_1, eth::Asset::Eth), INIT_BOOSTER_ETH_BALANCE);
			}
		});
	}
}

#[test]
fn stop_boosting() {
	new_test_ext().execute_with(|| {
		const BOOSTER_AMOUNT_1: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;

		setup();

		assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE);

		assert_ok!(IngressEgress::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			eth::Asset::Eth,
			BOOSTER_AMOUNT_1,
			BoostPoolTier::TenBps
		));

		let (_channel_id, deposit_address) = request_deposit_address_eth(LP_ACCOUNT, 30);
		let _deposit_id = prewitness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);

		assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE - BOOSTER_AMOUNT_1);

		// Booster stops boosting and get the available portion of their funds immediately:
		assert_ok!(IngressEgress::stop_boosting(
			RuntimeOrigin::signed(BOOSTER_1),
			eth::Asset::Eth,
			BoostPoolTier::TenBps
		));

		const BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT / 1000;
		const AVAILABLE_BOOST_AMOUNT: AssetAmount = BOOSTER_AMOUNT_1 - (DEPOSIT_AMOUNT - BOOST_FEE);
		assert_eq!(
			get_lp_eth_balance(&BOOSTER_1),
			INIT_BOOSTER_ETH_BALANCE - BOOSTER_AMOUNT_1 + AVAILABLE_BOOST_AMOUNT
		);

		// Deposit is finalised, the booster gets their remaining funds from the pool:
		witness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);
		assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE + BOOST_FEE);
	});
}

#[track_caller]
fn assert_boosted(
	deposit_address: H160,
	boost_id: BoostId,
	pools: impl IntoIterator<Item = BoostPoolTier>,
) {
	assert_eq!(
		DepositChannelLookup::<Test>::get(deposit_address).unwrap().boost_status,
		BoostStatus::Boosted { boost_id, pools: Vec::from_iter(pools.into_iter()) }
	);
}

#[track_caller]
fn assert_not_boosted(deposit_address: H160) {
	assert_eq!(
		DepositChannelLookup::<Test>::get(deposit_address).unwrap().boost_status,
		BoostStatus::NotBoosted
	);
}

#[test]
fn witnessed_amount_does_not_match_boosted() {
	new_test_ext().execute_with(|| {
		const BOOSTER_AMOUNT_1: AssetAmount = 500_000_000;
		const PREWITNESSED_DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const WITNESSED_DEPOSIT_AMOUNT: AssetAmount = PREWITNESSED_DEPOSIT_AMOUNT + 1;

		setup();

		assert_ok!(IngressEgress::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			eth::Asset::Eth,
			BOOSTER_AMOUNT_1,
			BoostPoolTier::FiveBps
		));

		assert_eq!(get_available_amount(eth::Asset::Eth, BoostPoolTier::FiveBps), BOOSTER_AMOUNT_1);

		// ==== LP sends funds to liquidity deposit address, which gets pre-witnessed ====
		let (channel_id, deposit_address) = request_deposit_address_eth(LP_ACCOUNT, 30);
		let deposit_id =
			prewitness_deposit(deposit_address, eth::Asset::Eth, PREWITNESSED_DEPOSIT_AMOUNT);

		const BOOST_FEE: AssetAmount = PREWITNESSED_DEPOSIT_AMOUNT / 2000;

		assert_boosted(deposit_address, deposit_id, [BoostPoolTier::FiveBps]);

		assert_eq!(
			get_lp_eth_balance(&LP_ACCOUNT),
			PREWITNESSED_DEPOSIT_AMOUNT - BOOST_FEE - INGRESS_FEE
		);

		assert_eq!(
			get_available_amount(eth::Asset::Eth, BoostPoolTier::FiveBps),
			BOOSTER_AMOUNT_1 - PREWITNESSED_DEPOSIT_AMOUNT + BOOST_FEE
		);

		// Witnessing incorrect amount does not lead to booster pools getting credited,
		// and is instead processed as usual (crediting the LP in this case):
		witness_deposit(deposit_address, eth::Asset::Eth, WITNESSED_DEPOSIT_AMOUNT);
		assert_eq!(
			get_available_amount(eth::Asset::Eth, BoostPoolTier::FiveBps),
			BOOSTER_AMOUNT_1 - PREWITNESSED_DEPOSIT_AMOUNT + BOOST_FEE
		);

		assert_boosted(deposit_address, deposit_id, [BoostPoolTier::FiveBps]);

		assert_eq!(
			get_lp_eth_balance(&LP_ACCOUNT),
			PREWITNESSED_DEPOSIT_AMOUNT + WITNESSED_DEPOSIT_AMOUNT - BOOST_FEE - 2 * INGRESS_FEE
		);

		// Check that receiving unexpected amount didn't affect our ability to finalise the boost
		// when the correct amount is received after all:
		witness_deposit(deposit_address, eth::Asset::Eth, PREWITNESSED_DEPOSIT_AMOUNT);
		assert_eq!(PrewitnessedDeposits::<Test>::get(channel_id, deposit_id), None);
		assert_eq!(
			get_available_amount(eth::Asset::Eth, BoostPoolTier::FiveBps),
			BOOSTER_AMOUNT_1 + BOOST_FEE
		);

		// The channel should no longer be boosted:
		assert_not_boosted(deposit_address);

		// Now that the boost has been finalised, the next deposit can be boosted again:
		{
			let deposit_id =
				prewitness_deposit(deposit_address, eth::Asset::Eth, WITNESSED_DEPOSIT_AMOUNT);
			assert_boosted(deposit_address, deposit_id, [BoostPoolTier::FiveBps]);
		}
	});
}

#[test]
fn double_prewitness_due_to_reorg() {
	new_test_ext().execute_with(|| {
		const BOOSTER_AMOUNT_1: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const BOOST_FEE_BPS: BasisPoints = 10;
		const BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000;

		setup();

		assert_ok!(IngressEgress::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			eth::Asset::Eth,
			BOOSTER_AMOUNT_1,
			BoostPoolTier::TenBps
		));

		// ==== LP sends funds to liquidity deposit address, which gets pre-witnessed ====
		let (channel_id, deposit_address) = request_deposit_address_eth(LP_ACCOUNT, BOOST_FEE_BPS);
		let deposit_id1 = prewitness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);

		const LP_BALANCE_AFTER_BOOST: AssetAmount =
			INIT_LP_BALANCE + DEPOSIT_AMOUNT - BOOST_FEE - INGRESS_FEE;
		const AVAILABLE_AMOUNT_AFTER_BOOST: AssetAmount =
			BOOSTER_AMOUNT_1 - DEPOSIT_AMOUNT + BOOST_FEE;

		// First deposit should be boosted, crediting the LP as per channel action:
		{
			assert_boosted(deposit_address, deposit_id1, [BoostPoolTier::TenBps]);

			assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), LP_BALANCE_AFTER_BOOST);

			assert_eq!(
				get_available_amount(eth::Asset::Eth, BoostPoolTier::TenBps),
				AVAILABLE_AMOUNT_AFTER_BOOST
			);
		}

		// Due to reorg, the same deposit is pre-witnessed again, but it has no effect since
		// we don't boost it due to an existing boost:
		let deposit_id2 = prewitness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);
		{
			// No channel action took place, LP balance is unchanged:
			assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), LP_BALANCE_AFTER_BOOST);

			// No funds from the boost pool are consumed:
			assert_eq!(
				get_available_amount(eth::Asset::Eth, BoostPoolTier::TenBps),
				AVAILABLE_AMOUNT_AFTER_BOOST
			);
		}

		// The deposit is finally fully witnessed, it has no effect on the LP, but
		// boosters get credited
		{
			witness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);

			assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), LP_BALANCE_AFTER_BOOST);

			assert_eq!(
				get_available_amount(eth::Asset::Eth, BoostPoolTier::TenBps),
				BOOSTER_AMOUNT_1 + BOOST_FEE
			);

			// One prewitness deposit is "consumed", but one is still pending:
			assert_eq!(PrewitnessedDeposits::<Test>::get(channel_id, deposit_id1), None);
			assert!(PrewitnessedDeposits::<Test>::get(channel_id, deposit_id2).is_some());
		}

		// When the channel expires, the redundant prewitnessed deposit is finally removed:
		{
			let recycle_block = IngressEgress::expiry_and_recycle_block_height().2;
			BlockHeightProvider::<MockEthereum>::set_block_height(recycle_block);
			IngressEgress::on_idle(recycle_block, Weight::MAX);
			assert_eq!(PrewitnessedDeposits::<Test>::get(channel_id, deposit_id2), None);
		}
	});
}

#[test]
fn zero_boost_fee_deposit() {
	new_test_ext().execute_with(|| {
		const BOOSTER_AMOUNT: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;

		setup();

		assert_ok!(IngressEgress::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			eth::Asset::Eth,
			BOOSTER_AMOUNT,
			BoostPoolTier::TenBps
		));

		let (_channel_id, deposit_address) = request_deposit_address_eth(LP_ACCOUNT, 0);
		let _deposit_id = prewitness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);

		// The deposit is pre-witnessed, but no channel action took place due to 0 boost fee:
		{
			assert_not_boosted(deposit_address);
			assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), INIT_LP_BALANCE);
		}

		// When the deposit is finalised, it is processed as normal:
		{
			witness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);
			assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), DEPOSIT_AMOUNT - INGRESS_FEE);
		}
	});
}

#[test]
fn skip_zero_amount_pool() {
	// 10 bps has 0 available funds, but we are able to skip it and
	// boost with the next tier pool

	new_test_ext().execute_with(|| {
		const POOL_AMOUNT: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 1_000_000_000;

		setup();

		assert_ok!(IngressEgress::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			eth::Asset::Eth,
			POOL_AMOUNT,
			BoostPoolTier::FiveBps
		));

		assert_ok!(IngressEgress::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_2),
			eth::Asset::Eth,
			POOL_AMOUNT,
			BoostPoolTier::ThirtyBps
		));

		let (_channel_id, deposit_address) = request_deposit_address_eth(LP_ACCOUNT, 50);
		let deposit_id = prewitness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);

		// Should be able to boost without the 30bps pool:
		assert_boosted(
			deposit_address,
			deposit_id,
			[BoostPoolTier::FiveBps, BoostPoolTier::ThirtyBps],
		);
		assert!(get_lp_eth_balance(&LP_ACCOUNT) > INIT_LP_BALANCE);
	});
}

#[test]
fn insufficient_funds_for_boost() {
	new_test_ext().execute_with(|| {
		const BOOSTER_AMOUNT: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 1_000_000_000;

		setup();

		assert_ok!(IngressEgress::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			eth::Asset::Eth,
			BOOSTER_AMOUNT,
			BoostPoolTier::FiveBps
		));

		let (_channel_id, deposit_address) = request_deposit_address_eth(LP_ACCOUNT, 10);
		let _deposit_id = prewitness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);

		// The deposit is pre-witnessed, but no channel action took place:
		{
			assert_not_boosted(deposit_address);
			assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), INIT_LP_BALANCE);
		}

		// When the deposit is finalised, it is processed as normal:
		{
			witness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);
			assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), DEPOSIT_AMOUNT - INGRESS_FEE);
		}
	});
}

#[test]
fn lost_funds_are_acknowledged_by_boost_pool() {
	new_test_ext().execute_with(|| {
		const BOOSTER_AMOUNT: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const BOOST_FEE_BPS: BasisPoints = BoostPoolTier::FiveBps as u16;
		const BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000;

		setup();

		assert_ok!(IngressEgress::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			eth::Asset::Eth,
			BOOSTER_AMOUNT,
			BoostPoolTier::FiveBps
		));

		let (channel_id, deposit_address) = request_deposit_address_eth(LP_ACCOUNT, BOOST_FEE_BPS);

		let deposit_id = prewitness_deposit(deposit_address, eth::Asset::Eth, DEPOSIT_AMOUNT);

		assert_eq!(
			DepositChannelLookup::<Test>::get(deposit_address).unwrap().boost_status,
			BoostStatus::Boosted { boost_id: deposit_id, pools: vec![BoostPoolTier::FiveBps] }
		);

		assert_eq!(get_lp_eth_balance(&LP_ACCOUNT), DEPOSIT_AMOUNT - BOOST_FEE - INGRESS_FEE);

		assert_eq!(
			BoostPools::<Test>::get(eth::Asset::Eth, BoostPoolTier::FiveBps)
				.unwrap()
				.get_pending_boosts(),
			vec![deposit_id]
		);

		// When the channel expires, the record holding amounts owed to boosters
		// from the deposit is cleared:
		{
			let recycle_block = IngressEgress::expiry_and_recycle_block_height().2;
			BlockHeightProvider::<MockEthereum>::set_block_height(recycle_block);
			IngressEgress::on_idle(recycle_block, Weight::MAX);
			assert_eq!(PrewitnessedDeposits::<Test>::get(channel_id, deposit_id), None);

			assert_eq!(
				BoostPools::<Test>::get(eth::Asset::Eth, BoostPoolTier::FiveBps)
					.unwrap()
					.get_pending_boosts(),
				Vec::<BoostId>::new()
			);
		}
	});
}
