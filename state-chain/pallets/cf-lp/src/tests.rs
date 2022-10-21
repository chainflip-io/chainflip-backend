use crate::{mock::*, FreeBalances};

use cf_primitives::{
	liquidity::AmmRange, AccountRole, Asset, ForeignChain, ForeignChainAddress, ForeignChainAsset,
	TradingPosition,
};
use cf_traits::{
	mocks::system_state_info::MockSystemStateInfo, AccountRoleRegistry, SystemStateInfo,
};
use frame_support::{assert_noop, assert_ok, error::BadOrigin, traits::OnNewAccount};

const ALICE: u64 = 1;

#[test]
fn only_liquidity_provider_can_manage_positions() {
	new_test_ext().execute_with(|| {
		let position = TradingPosition::ClassicV3 {
			range: AmmRange { lower: 0, upper: 0 },
			volume_0: 100,
			volume_1: 1000,
		};
		let pool_id = (Asset::Eth, Asset::Usdc);

		AccountRoles::on_new_account(&ALICE);
		assert_ok!(<AccountRoles as AccountRoleRegistry<Test>>::register_account_role(
			&ALICE,
			AccountRole::None
		));
		assert_ok!(LiquidityProvider::add_liquidity_pool(Origin::root(), pool_id.0, pool_id.1));
		assert_ok!(LiquidityProvider::set_liquidity_pool_status(
			Origin::root(),
			pool_id.0,
			pool_id.1,
			true
		));

		assert_noop!(
			LiquidityProvider::open_position(Origin::signed(ALICE), pool_id, position,),
			BadOrigin,
		);

		assert_noop!(
			LiquidityProvider::update_position(Origin::signed(ALICE), pool_id, 0, position,),
			BadOrigin,
		);

		assert_noop!(LiquidityProvider::close_position(Origin::signed(ALICE), 0), BadOrigin,);
	});
}

#[test]
fn egress_chain_and_asset_must_match() {
	new_test_ext().execute_with(|| {
		AccountRoles::on_new_account(&ALICE);
		assert_ok!(<AccountRoles as AccountRoleRegistry<Test>>::register_account_role(
			&ALICE,
			AccountRole::LiquidityProvider
		));

		assert_noop!(
			LiquidityProvider::withdraw_liquidity(
				Origin::signed(ALICE),
				1,
				ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
				ForeignChainAddress::Dot([0x00; 32]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);
		assert_noop!(
			LiquidityProvider::withdraw_liquidity(
				Origin::signed(ALICE),
				1,
				ForeignChainAsset { chain: ForeignChain::Polkadot, asset: Asset::Dot },
				ForeignChainAddress::Eth([0x00; 20]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);
	});
}

#[test]
fn liquidity_providers_can_withdraw_liquidity() {
	new_test_ext().execute_with(|| {
		AccountRoles::on_new_account(&ALICE);
		assert_ok!(<AccountRoles as AccountRoleRegistry<Test>>::register_account_role(
			&ALICE,
			AccountRole::LiquidityProvider
		));
		FreeBalances::<Test>::insert(ALICE, Asset::Eth, 1_000);

		assert!(!IsValid::get());
		assert_noop!(
			LiquidityProvider::withdraw_liquidity(
				Origin::signed(ALICE),
				100,
				ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
				ForeignChainAddress::Eth([0x00; 20]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);

		IsValid::set(true);
		assert!(LastEgress::get().is_none());
		assert_ok!(LiquidityProvider::withdraw_liquidity(
			Origin::signed(ALICE),
			100,
			ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
			ForeignChainAddress::Eth([0x00; 20]),
		));
		assert_eq!(
			LastEgress::get(),
			Some((
				ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
				100,
				ForeignChainAddress::Eth([0x00; 20])
			))
		);
	});
}

#[test]
fn cannot_deposit_and_withdrawal_during_maintenance() {
	new_test_ext().execute_with(|| {
		// Setup account for ALICE
		AccountRoles::on_new_account(&ALICE);
		assert_ok!(<AccountRoles as AccountRoleRegistry<_>>::register_account_role(
			&ALICE,
			AccountRole::LiquidityProvider
		));
		FreeBalances::<Test>::insert(ALICE, Asset::Eth, 1_000);
		IsValid::set(true);

		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		assert!(MockSystemStateInfo::is_maintenance_mode());

		// Cannot request deposit address during maintenance.
		assert_noop!(
			LiquidityProvider::request_deposit_address(
				Origin::signed(ALICE),
				ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
			),
			"We are in maintenance!"
		);

		// Cannot withdraw liquidity during maintenance.
		assert_noop!(
			LiquidityProvider::withdraw_liquidity(
				Origin::signed(ALICE),
				100,
				ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
				ForeignChainAddress::Eth([0x00; 20]),
			),
			"We are in maintenance!"
		);

		// Deactivate maintenance mode
		MockSystemStateInfo::set_maintenance(false);
		assert!(!MockSystemStateInfo::is_maintenance_mode());

		// Deposit and withdrawal can now work as per normal.
		assert_ok!(LiquidityProvider::request_deposit_address(
			Origin::signed(ALICE),
			ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
		));

		assert_ok!(LiquidityProvider::withdraw_liquidity(
			Origin::signed(ALICE),
			100,
			ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
			ForeignChainAddress::Eth([0x00; 20]),
		));
	});
}

#[test]
fn cannot_manage_liquidity_during_maintenance() {
	new_test_ext().execute_with(|| {
		// Setup account and liquidity pool
		AccountRoles::on_new_account(&ALICE);
		assert_ok!(<AccountRoles as AccountRoleRegistry<_>>::register_account_role(
			&ALICE,
			AccountRole::LiquidityProvider
		));
		FreeBalances::<Test>::insert(ALICE, Asset::Eth, 1_000_000);
		FreeBalances::<Test>::insert(ALICE, Asset::Usdc, 1_000_000);
		IsValid::set(true);

		let position = TradingPosition::ClassicV3 {
			range: AmmRange { lower: 0, upper: 0 },
			volume_0: 100,
			volume_1: 1000,
		};
		let pool_id = (Asset::Eth, Asset::Usdc);

		assert_ok!(LiquidityProvider::add_liquidity_pool(Origin::root(), pool_id.0, pool_id.1));
		assert_ok!(LiquidityProvider::set_liquidity_pool_status(
			Origin::root(),
			pool_id.0,
			pool_id.1,
			true
		));

		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		assert!(MockSystemStateInfo::is_maintenance_mode());

		assert_noop!(
			LiquidityProvider::open_position(Origin::signed(ALICE), pool_id, position,),
			"We are in maintenance!"
		);
		assert_noop!(
			LiquidityProvider::update_position(Origin::signed(ALICE), pool_id, 0, position,),
			"We are in maintenance!"
		);
		assert_noop!(
			LiquidityProvider::close_position(Origin::signed(ALICE), 0,),
			"We are in maintenance!"
		);

		// Deactivate maintenance mode
		MockSystemStateInfo::set_maintenance(false);
		assert!(!MockSystemStateInfo::is_maintenance_mode());

		assert_ok!(LiquidityProvider::open_position(Origin::signed(ALICE), pool_id, position,),);
		assert_ok!(
			LiquidityProvider::update_position(Origin::signed(ALICE), pool_id, 0, position,),
		);
		assert_ok!(LiquidityProvider::close_position(Origin::signed(ALICE), 0,),);
	});
}
