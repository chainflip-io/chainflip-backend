use crate as pallet_cf_refunding;
use crate::PalletSafeMode;
use cf_chains::{dot::PolkadotAccountId, AnyChain};
use cf_primitives::AccountId;

use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode, mocks::egress_handler::MockEgressHandler,
};
use frame_support::{derive_impl, parameter_types, sp_runtime::app_crypto::sp_core::H160};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	Permill,
};

use cf_chains::ForeignChainAddress;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Refunding: pallet_cf_refunding,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
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
	pub const NetworkFee: Permill = Permill::from_percent(0);
}

pub const ETH_ADDR_1: ForeignChainAddress = ForeignChainAddress::Eth(H160([0; 20]));
pub const ETH_ADDR_2: ForeignChainAddress = ForeignChainAddress::Eth(H160([1; 20]));
pub const ETH_ADDR_3: ForeignChainAddress = ForeignChainAddress::Eth(H160([2; 20]));

pub const DOT_ADDR_1: ForeignChainAddress =
	ForeignChainAddress::Dot(PolkadotAccountId::from_aliased([1; 32]));

impl_mock_runtime_safe_mode!(refunding: PalletSafeMode);

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EgressHandler = MockEgressHandler<AnyChain>;
	type SafeMode = MockRuntimeSafeMode;
}

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
	}
}
