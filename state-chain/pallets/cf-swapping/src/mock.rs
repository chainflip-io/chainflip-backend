use crate::{self as pallet_cf_swapping, WeightInfo};
use cf_chains::AnyChain;
use cf_primitives::{Asset, AssetAmount};
use cf_traits::{
	mocks::{
		egress_handler::MockEgressHandler, ensure_origin_mock::NeverFailingOriginCheck,
		ingress_handler::MockIngressHandler, system_state_info::MockSystemStateInfo,
	},
	Chainflip, SwappingApi,
};
use frame_support::{dispatch::DispatchError, parameter_types, weights::Weight};
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
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
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

pub struct MockSwappingApi;

impl SwappingApi for MockSwappingApi {
	fn swap(
		_from: Asset,
		_to: Asset,
		swap_input: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		Ok(swap_input)
	}
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

pub struct MockWeightInfo;

impl WeightInfo for MockWeightInfo {
	fn register_swap_intent() -> Weight {
		Weight::from_ref_time(100)
	}

	fn on_idle() -> Weight {
		Weight::from_ref_time(100)
	}

	fn execute_group_of_swaps(_a: u32) -> Weight {
		Weight::from_ref_time(100)
	}

	fn withdraw() -> Weight {
		Weight::from_ref_time(100)
	}
}

impl pallet_cf_swapping::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type AccountRoleRegistry = ();
	type IngressHandler = MockIngressHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type WeightInfo = MockWeightInfo;
	type SwappingApi = MockSwappingApi;
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
