use crate::{self as pallet_cf_swapping, PalletSafeMode, WeightInfo};
use cf_chains::AnyChain;
use cf_primitives::{Asset, AssetAmount};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
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
	traits::{BlakeTwo256, IdentityLookup},
	Percent,
};

type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
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
impl_mock_runtime_safe_mode! { swapping: PalletSafeMode }

parameter_types! {
	pub static NetworkFee: Percent = Percent::from_percent(0);
	pub static Swaps: Vec<(Asset, Asset, AssetAmount)> = vec![];
	pub static SwapRate: f64 = 1f64;
}
pub struct MockSwappingApi;
impl SwappingApi for MockSwappingApi {
	fn take_network_fee(input_amount: AssetAmount) -> AssetAmount {
		input_amount - NetworkFee::get() * input_amount
	}

	fn swap_single_leg(
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		let mut swaps = Swaps::get();
		swaps.push((from, to, input_amount));
		Swaps::set(swaps);
		Ok((input_amount as f64 * SwapRate::get()) as AssetAmount)
	}
}

pub struct MockWeightInfo;

impl WeightInfo for MockWeightInfo {
	fn request_swap_deposit_address() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn withdraw() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn schedule_swap_from_contract() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn ccm_deposit() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn register_as_broker() -> Weight {
		Weight::from_parts(100, 0)
	}

	fn set_maximum_swap_amount() -> Weight {
		Weight::from_parts(100, 0)
	}
}

impl pallet_cf_swapping::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type DepositHandler = MockDepositHandler<AnyChain, Self>;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type AddressConverter = MockAddressConverter;
	type SwappingApi = MockSwappingApi;
	type SafeMode = MockRuntimeSafeMode;
	type WeightInfo = MockWeightInfo;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
	},
	|| {
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(&ALICE).unwrap();
	},
}
