use crate::mock::*;
use cf_primitives::{liquidity::AmmRange, AccountRole, Asset, TradingPosition};
use cf_traits::AccountRoleRegistry;
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

		AccountRegistry::on_new_account(&ALICE);
		assert_ok!(AccountRegistry::register_account_role(&ALICE, AccountRole::None));
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
