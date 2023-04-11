pub use crate::{self as pallet_cf_ingress_egress};
pub use cf_chains::{
	eth::api::{EthereumApi, EthereumReplayProtection},
	Chain, ChainAbi, ChainEnvironment,
};
pub use cf_primitives::{
	chains::{assets, Ethereum},
	Asset, AssetAmount, EthereumAddress, ExchangeRate, ETHEREUM_ETH_ADDRESS,
};
use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};

use frame_support::traits::UnfilteredDispatchable;

use cf_traits::{
	impl_mock_callback,
	mocks::api_call::{MockEthEnvironment, MockEthereumApiCall},
	IngressHandler,
};

pub use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, system_state_info::MockSystemStateInfo},
	Broadcaster,
};
use frame_support::{instances::Instance1, parameter_types, traits::ConstU64};
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

pub struct MockBroadcast;
impl Broadcaster<Ethereum> for MockBroadcast {
	type ApiCall = MockEthereumApiCall<MockEthEnvironment>;
	type Callback = RuntimeCall;

	fn threshold_sign_and_broadcast(
		_api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		(1, 2)
	}

	fn threshold_sign_and_broadcast_with_callback(
		_api_call: Self::ApiCall,
		callback: Self::Callback,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		// TODO: Call the callback.
		let _ = callback.dispatch_bypass_filter(frame_system::RawOrigin::Root.into());
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
	type IntentTTL = ConstU64<5_u64>;
	type IngressHandler = MockIngressHandler;
	type WeightInfo = ();
	type CcmHandler = ();
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
