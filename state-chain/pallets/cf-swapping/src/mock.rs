use crate::{self as pallet_cf_swapping};
use cf_primitives::{AssetAmount, ForeignChainAddress, ForeignChainAsset};
use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, system_state_info::MockSystemStateInfo},
	AmmPoolApi, Chainflip, EgressApi, IngressApi,
};
use frame_support::parameter_types;
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
type Balance = u128;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		Swapping: pallet_cf_swapping,
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
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
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

pub struct MockIngress;

impl IngressApi for MockIngress {
	type AccountId = AccountId;

	fn register_liquidity_ingress_intent(
		_lp_account: Self::AccountId,
		_ingress_asset: ForeignChainAsset,
	) -> Result<(u64, cf_primitives::ForeignChainAddress), sp_runtime::DispatchError> {
		Ok((0, ForeignChainAddress::Eth(Default::default())))
	}

	fn register_swap_intent(
		_ingress_asset: ForeignChainAsset,
		_schedule_egress: ForeignChainAsset,
		_egress_address: ForeignChainAddress,
		_relayer_commission_bps: u16,
	) -> Result<(u64, cf_primitives::ForeignChainAddress), sp_runtime::DispatchError> {
		Ok((0, ForeignChainAddress::Eth(Default::default())))
	}
}

pub struct MockEgressApi;
impl EgressApi for MockEgressApi {
	fn schedule_egress(
		_foreign_asset: ForeignChainAsset,
		_amount: AssetAmount,
		_egress_address: ForeignChainAddress,
	) {
	}

	fn is_egress_valid(
		_foreign_asset: &ForeignChainAsset,
		_egress_address: &ForeignChainAddress,
	) -> bool {
		true
	}
}

pub struct MockAmmPoolApi;

impl AmmPoolApi for MockAmmPoolApi {
	type Balance = Balance;

	fn asset_0(&self) -> cf_primitives::Asset {
		todo!()
	}

	fn asset_1(&self) -> cf_primitives::Asset {
		todo!()
	}

	fn liquidity_0(&self) -> Self::Balance {
		todo!()
	}

	fn liquidity_1(&self) -> Self::Balance {
		todo!()
	}

	fn get_exchange_rate(&self) -> cf_primitives::ExchangeRate {
		todo!()
	}

	fn get_liquidity_requirement(
		&self,
		_position: &cf_primitives::TradingPosition<Self::Balance>,
	) -> Option<(Self::Balance, Self::Balance)> {
		todo!()
	}

	fn swap(
		_ingress_asset: cf_primitives::Asset,
		_egress_asset: ForeignChainAsset,
		_ingress_amount: Self::Balance,
	) -> Self::Balance {
		Self::Balance::default()
	}
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

impl pallet_cf_swapping::Config for Test {
	type Event = Event;
	type Ingress = MockIngress;
	type AccountRoleRegistry = ();
	type Egress = MockEgressApi;
	type AmmPoolApi = MockAmmPoolApi;
	type WeightInfo = ();
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig { system: Default::default() };

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
