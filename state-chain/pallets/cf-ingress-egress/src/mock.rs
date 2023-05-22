pub use crate::{self as pallet_cf_ingress_egress};
pub use cf_chains::{
	address::ForeignChainAddress,
	eth::api::{EthereumApi, EthereumReplayProtection},
	CcmDepositMetadata, Chain, ChainAbi, ChainEnvironment,
};
pub use cf_primitives::{
	chains::{assets, Ethereum},
	Asset, AssetAmount, EthereumAddress, ETHEREUM_ETH_ADDRESS,
};
use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};

use frame_support::{instances::Instance1, parameter_types, traits::UnfilteredDispatchable};

pub use cf_traits::Broadcaster;
use cf_traits::{
	impl_mock_callback, impl_mock_chainflip,
	mocks::{
		api_call::{MockEthEnvironment, MockEthereumApiCall},
		broadcaster::MockBroadcaster,
		ccm_handler::MockCcmHandler,
	},
	DepositHandler,
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
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
		IngressEgress: pallet_cf_ingress_egress,
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
impl_mock_callback!(RuntimeOrigin);

parameter_types! {
	pub static EgressedApiCalls: Vec<MockEthereumApiCall<MockEthEnvironment>> = Default::default();
}

pub struct MockDepositHandler;
impl DepositHandler<Ethereum> for MockDepositHandler {}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Ethereum;
	type AddressDerivation = ();
	type LpBalance = Self;
	type SwapDepositHandler = Self;
	type ChainApiCall = MockEthereumApiCall<MockEthEnvironment>;
	type Broadcaster = MockBroadcaster<(Self::ChainApiCall, RuntimeCall)>;
	type DepositHandler = MockDepositHandler;
	type WeightInfo = ();
	type CcmHandler = MockCcmHandler;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> cf_test_utilities::TestExternalities<Test, AllPalletsWithSystem> {
	cf_test_utilities::TestExternalities::<_, _>::new(GenesisConfig { system: Default::default() })
}
