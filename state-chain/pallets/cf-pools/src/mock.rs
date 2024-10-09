use crate::{self as pallet_cf_pools, PalletSafeMode};
use cf_chains::{assets::any::AssetMap, Ethereum};
use cf_primitives::{Asset, AssetAmount};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		balance_api::MockLpRegistration, egress_handler::MockEgressHandler,
		swap_request_api::MockSwapRequestHandler,
	},
	AccountRoleRegistry, BalanceApi,
};
use frame_support::{derive_impl, parameter_types};
use frame_system as system;
use sp_runtime::DispatchResult;
use sp_std::collections::btree_map::BTreeMap;

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 124u64;

type AccountId = u64;
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

parameter_types! {
	pub static AliceCollectedEth: AssetAmount = Default::default();
	pub static AliceCollectedUsdc: AssetAmount = Default::default();
	pub static BobCollectedEth: AssetAmount = Default::default();
	pub static BobCollectedUsdc: AssetAmount = Default::default();
	pub static AliceDebitedEth: AssetAmount = Default::default();
	pub static AliceDebitedUsdc: AssetAmount = Default::default();
	pub static BobDebitedEth: AssetAmount = Default::default();
	pub static BobDebitedUsdc: AssetAmount = Default::default();
	pub static RecordedFees: BTreeMap<AccountId, (Asset, AssetAmount)> = BTreeMap::new();
}
pub struct MockBalance;
impl BalanceApi for MockBalance {
	type AccountId = AccountId;

	fn try_credit_account(
		who: &Self::AccountId,
		asset: cf_primitives::Asset,
		amount: cf_primitives::AssetAmount,
	) -> DispatchResult {
		match (*who, asset) {
			(ALICE, Asset::Eth) => AliceCollectedEth::set(AliceCollectedEth::get() + amount),
			(ALICE, Asset::Usdc) => AliceCollectedUsdc::set(AliceCollectedUsdc::get() + amount),
			(BOB, Asset::Eth) => BobCollectedEth::set(BobCollectedEth::get() + amount),
			(BOB, Asset::Usdc) => BobCollectedUsdc::set(BobCollectedUsdc::get() + amount),
			_ => (),
		}
		Ok(())
	}

	fn try_debit_account(
		who: &Self::AccountId,
		asset: cf_primitives::Asset,
		amount: cf_primitives::AssetAmount,
	) -> sp_runtime::DispatchResult {
		match (*who, asset) {
			(ALICE, Asset::Eth) => AliceDebitedEth::set(AliceDebitedEth::get() + amount),
			(ALICE, Asset::Usdc) => AliceDebitedUsdc::set(AliceDebitedUsdc::get() + amount),
			(BOB, Asset::Eth) => BobDebitedEth::set(BobDebitedEth::get() + amount),
			(BOB, Asset::Usdc) => BobDebitedUsdc::set(BobDebitedUsdc::get() + amount),
			_ => (),
		}
		Ok(())
	}

	fn free_balances(_who: &Self::AccountId) -> AssetMap<AssetAmount> {
		unimplemented!()
	}

	fn get_balance(_who: &Self::AccountId, _asset: Asset) -> AssetAmount {
		unimplemented!()
	}
}

impl MockBalance {
	pub fn assert_fees_recorded(who: &AccountId) {
		assert!(RecordedFees::get().contains_key(who), "Fees not recorded for {:?}", who);
	}
}

impl_mock_runtime_safe_mode!(pools: PalletSafeMode);
impl pallet_cf_pools::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type LpBalance = MockBalance;
	type SwapRequestHandler = MockSwapRequestHandler<(Ethereum, MockEgressHandler<Ethereum>)>;
	type LpRegistrationApi = MockLpRegistration;
	type SafeMode = MockRuntimeSafeMode;
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
