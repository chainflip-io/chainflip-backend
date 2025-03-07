use crate::{self as pallet_cf_pools, PalletSafeMode};
use cf_chains::Ethereum;
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		balance_api::MockLpRegistration, egress_handler::MockEgressHandler,
		swap_request_api::MockSwapRequestHandler,
		trading_strategy_limits::MockTradingStrategyParameters,
	},
	AccountRoleRegistry,
};
use frame_support::derive_impl;
use frame_system as system;

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 124u64;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		LiquidityPools: pallet_cf_pools,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

impl_mock_runtime_safe_mode!(pools: PalletSafeMode);
impl pallet_cf_pools::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type LpBalance = cf_traits::mocks::balance_api::MockBalance;
	type SwapRequestHandler = MockSwapRequestHandler<(Ethereum, MockEgressHandler<Ethereum>)>;
	type LpRegistrationApi = MockLpRegistration;
	type SafeMode = MockRuntimeSafeMode;
	type TradingStrategyParameters = MockTradingStrategyParameters;
	type WeightInfo = ();
}

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		frame_support::assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&ALICE,
		));
		frame_support::assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&BOB,
		));
	}
}
