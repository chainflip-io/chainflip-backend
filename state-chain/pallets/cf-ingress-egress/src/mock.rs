pub use crate::{self as pallet_cf_ingress_egress};
pub use cf_chains::{
	eth::api::{EthereumApi, EthereumReplayProtection},
	Chain, ChainAbi, ChainEnvironment,
};
use cf_chains::{eth::EthereumIngressId, IngressTypeGeneration};
use cf_primitives::BroadcastId;
pub use cf_primitives::{
	chains::{assets, Ethereum},
	Asset, AssetAmount, EthereumAddress, ExchangeRate, ETHEREUM_ETH_ADDRESS,
};

use cf_traits::mocks::{
	all_batch::{MockAllBatch, MockEthEnvironment},
	time_source,
};
pub use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, system_state_info::MockSystemStateInfo},
	Broadcaster,
};
use frame_support::{instances::Instance1, parameter_types, traits::ConstU64};
use frame_system as system;
use sp_core::{H160, H256};
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

pub struct MockIngressTypeGenerator;

impl IngressTypeGeneration for MockIngressTypeGenerator {
	type IngressType = EthereumIngressId;
	type Address = H160;

	fn generate_ingress_type(
		intent_id: u64,
		_address: Self::Address,
		_deployed: bool,
	) -> Self::IngressType {
		Self::IngressType::UnDeployed(intent_id)
	}

	fn deployment_status(is_deployed: bool) -> cf_chains::DeploymentStatus {
		if is_deployed {
			cf_chains::DeploymentStatus::Deployed
		} else {
			cf_chains::DeploymentStatus::Undeployed
		}
	}
}

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
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

pub struct MockBroadcast;
impl Broadcaster<Ethereum> for MockBroadcast {
	type ApiCall = MockAllBatch<MockEthEnvironment>;

	fn threshold_sign_and_broadcast(_api_call: Self::ApiCall) -> BroadcastId {
		1
	}
}

impl crate::Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Ethereum;
	type AddressDerivation = ();
	type LpProvisioning = Self;
	type SwapIntentHandler = Self;
	type AllBatch = MockAllBatch<MockEthEnvironment>;
	type Broadcaster = MockBroadcast;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type WeightInfo = ();
	type TTL = ConstU64<5_u64>;
	type TimeSource = time_source::Mock;
	type IngressTypeGenerator = MockIngressTypeGenerator;
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
