use crate as pallet_cf_trading_strategy;
use cf_traits::{
	impl_mock_chainflip,
	mocks::{balance_api::MockLpRegistration, pool_api::MockPoolApi},
	AccountRoleRegistry,
};
use frame_support::derive_impl;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		TradingStrategyPallet: pallet_cf_trading_strategy
	}
);

impl_mock_chainflip!(Test);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlock<Test>;
}

impl pallet_cf_trading_strategy::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type BalanceApi = cf_traits::mocks::balance_api::MockBalance;
	type PoolApi = MockPoolApi;
	type LpRegistrationApi = MockLpRegistration;
}

pub const LP: <Test as frame_system::Config>::AccountId = 123u64;
pub const OTHER_LP: <Test as frame_system::Config>::AccountId = 234u64;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		frame_support::assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&LP,
		));
		frame_support::assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&OTHER_LP,
		));
	}
}
