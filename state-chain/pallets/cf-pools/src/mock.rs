use crate::{self as pallet_cf_pools, PalletSafeMode};
use cf_primitives::{Asset, AssetAmount};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode, AccountRoleRegistry, LpBalanceApi,
};
use frame_support::parameter_types;
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	DispatchResult, Permill,
};

type AccountId = u64;

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

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

impl_mock_chainflip!(Test);

parameter_types! {
	// 20 Basis Points
	pub static NetworkFee: Permill = Permill::from_perthousand(2);
	pub static AliceCollectedEth: AssetAmount = Default::default();
	pub static AliceCollectedUsdc: AssetAmount = Default::default();
	pub static BobCollectedEth: AssetAmount = Default::default();
	pub static BobCollectedUsdc: AssetAmount = Default::default();
}
pub struct MockBalance;
impl LpBalanceApi for MockBalance {
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
		_who: &Self::AccountId,
		_asset: cf_primitives::Asset,
		_amount: cf_primitives::AssetAmount,
	) -> sp_runtime::DispatchResult {
		Ok(())
	}
}

impl_mock_runtime_safe_mode!(pools: PalletSafeMode);
impl pallet_cf_pools::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type LpBalance = MockBalance;
	type NetworkFee = NetworkFee;
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
