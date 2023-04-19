pub use crate::{self as pallet_cf_ingress_egress};
pub use cf_chains::{
	address::ForeignChainAddress,
	eth::api::{EthereumApi, EthereumReplayProtection},
	CcmIngressMetadata, Chain, ChainAbi, ChainEnvironment,
};
pub use cf_primitives::{
	chains::{assets, Ethereum},
	Asset, AssetAmount, EthereumAddress, ExchangeRate, ETHEREUM_ETH_ADDRESS,
};
use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};

use frame_support::{
	instances::Instance1,
	parameter_types,
	traits::{OnFinalize, OnIdle, OnInitialize, UnfilteredDispatchable},
	weights::Weight,
};

use cf_traits::{
	impl_mock_callback,
	mocks::{
		api_call::{MockEthEnvironment, MockEthereumApiCall},
		ccm_handler::MockCcmHandler,
	},
	IngressHandler,
};

pub use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, system_state_info::MockSystemStateInfo},
	Broadcaster,
};
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
		IngressEgress: pallet_cf_ingress_egress::<Instance1>,
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

impl cf_traits::Chainflip for Test {
	type ValidatorId = u64;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

impl_mock_callback!(RuntimeOrigin);

parameter_types! {
	pub static EgressedApiCall: Option<MockEthereumApiCall<MockEthEnvironment>> = None;
}

pub struct MockBroadcast;
impl Broadcaster<Ethereum> for MockBroadcast {
	type ApiCall = MockEthereumApiCall<MockEthEnvironment>;
	type Callback = RuntimeCall;

	fn threshold_sign_and_broadcast(
		api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		EgressedApiCall::set(Some(api_call));
		(1, 2)
	}

	fn threshold_sign_and_broadcast_with_callback(
		_api_call: Self::ApiCall,
		_callback: Self::Callback,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		(1, 2)
	}
}

pub struct MockIngressHandler;
impl IngressHandler<Ethereum> for MockIngressHandler {}

impl crate::Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Ethereum;
	type AddressDerivation = ();
	type LpBalance = Self;
	type SwapIntentHandler = Self;
	type ChainApiCall = MockEthereumApiCall<MockEthEnvironment>;
	type Broadcaster = MockBroadcast;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type IngressHandler = MockIngressHandler;
	type WeightInfo = ();
	type CcmHandler = MockCcmHandler;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;

pub struct WrappedExternalites(sp_io::TestExternalities);

impl WrappedExternalites {
	pub fn new() -> Self {
		Self(GenesisConfig { system: Default::default() }.build_storage().unwrap().into())
	}

	pub fn execute_with<R>(&mut self, f: impl FnOnce() -> R) -> R {
		self.0.execute_with(|| {
			System::set_block_number(1);
			f()
		})
	}

	pub fn execute_as_block(mut self, block_number: u64, f: impl FnOnce()) -> Self {
		self.0.execute_with(|| {
			System::set_block_number(block_number);
			<AllPalletsWithSystem as OnInitialize<_>>::on_initialize(block_number);
			f();
			<AllPalletsWithSystem as OnIdle<_>>::on_idle(block_number, Weight::MAX);
			<AllPalletsWithSystem as OnFinalize<_>>::on_finalize(block_number);
		});
		self
	}
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> WrappedExternalites {
	WrappedExternalites::new()
}
