pub use crate::{self as pallet_cf_ingress_egress};
use cf_chains::ReplayProtectionProvider;
pub use cf_chains::{
	eth::api::{EthereumApi, EthereumReplayProtection},
	Chain, ChainAbi, ChainEnvironment,
};
pub use cf_primitives::{
	chains::{assets, Ethereum},
	Asset, AssetAmount, EthereumAddress, ExchangeRate, ETHEREUM_ETH_ADDRESS,
};

use cf_traits::mocks::all_batch::MockAllBatch;
pub use cf_traits::{
	mocks::{ensure_origin_mock::NeverFailingOriginCheck, system_state_info::MockSystemStateInfo},
	Broadcaster,
};
use frame_support::{instances::Instance1, parameter_types};
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

pub const ETHEREUM_FLIP_ADDRESS: EthereumAddress = [0x00; 20];
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

impl cf_traits::Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

pub const FAKE_KEYMAN_ADDR: [u8; 20] = [0xcf; 20];
pub const CHAIN_ID: u64 = 31337;
pub const COUNTER: u64 = 42;
impl ReplayProtectionProvider<Ethereum> for Test {
	fn replay_protection() -> <Ethereum as ChainAbi>::ReplayProtection {
		EthereumReplayProtection {
			key_manager_address: FAKE_KEYMAN_ADDR,
			chain_id: CHAIN_ID,
			nonce: COUNTER,
		}
	}
}

pub struct MockBroadcast;
impl Broadcaster<Ethereum> for MockBroadcast {
	type ApiCall = MockAllBatch;

	fn threshold_sign_and_broadcast(_api_call: Self::ApiCall) {}
}

pub struct MockEthEnvironment;

impl ChainEnvironment<<Ethereum as Chain>::ChainAsset, <Ethereum as Chain>::ChainAccount>
	for MockEthEnvironment
{
	fn lookup(
		asset: <Ethereum as Chain>::ChainAsset,
	) -> Result<<Ethereum as Chain>::ChainAccount, frame_support::error::LookupError> {
		Ok(match asset {
			assets::eth::Asset::Eth => ETHEREUM_ETH_ADDRESS.into(),
			assets::eth::Asset::Flip => ETHEREUM_FLIP_ADDRESS.into(),
			_ => todo!(),
		})
	}
}

impl crate::Config<Instance1> for Test {
	type Event = Event;
	type TargetChain = Ethereum;
	type AddressDerivation = ();
	type LpProvisioning = Self;
	type SwapIntentHandler = Self;
	type AllBatch = MockAllBatch;
	type Broadcaster = MockBroadcast;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig { system: Default::default() };

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
