use crate::{self as pallet_cf_swapping, WeightInfo};
use cf_chains::AnyChain;
use cf_primitives::{Asset, AssetAmount, SwapOutput};
use cf_traits::{
	impl_mock_chainflip,
	mocks::{
		address_converter::MockAddressConverter, deposit_handler::MockDepositHandler,
		egress_handler::MockEgressHandler,
	},
	AccountRoleRegistry, SwappingApi,
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

impl_mock_chainflip!(Test);

pub struct MockSwappingApi;

impl SwappingApi for MockSwappingApi {
	fn swap(
		_from: Asset,
		_to: Asset,
		swap_input: AssetAmount,
	) -> Result<SwapOutput, DispatchError> {
		Ok(swap_input.into())
	}
}

pub struct MockWeightInfo;

impl WeightInfo for MockWeightInfo {
	fn request_swap_deposit_address() -> Weight {
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

	fn schedule_swap_by_witnesser() -> Weight {
		Weight::from_ref_time(100)
	}

	fn ccm_deposit() -> Weight {
		Weight::from_ref_time(100)
	}

	fn register_as_relayer() -> Weight {
		Weight::from_ref_time(100)
	}

	fn on_initialize(_a: u32) -> Weight {
		Weight::from_ref_time(100)
	}

	fn set_swap_ttl() -> Weight {
		Weight::from_ref_time(100)
	}
}

parameter_types! {
	pub GasPrice: AssetAmount = 1_000;
}

impl pallet_cf_swapping::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type DepositHandler = MockDepositHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type WeightInfo = MockWeightInfo;
	type AddressConverter = MockAddressConverter;
	type SwappingApi = MockSwappingApi;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config =
		GenesisConfig { system: Default::default(), swapping: SwappingConfig { swap_ttl: 5 } };

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_relayer(&ALICE)
			.unwrap();
		System::set_block_number(1);
		System::set_block_number(1);
	});

	ext
}
