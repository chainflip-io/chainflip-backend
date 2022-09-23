use crate::{self as pallet_cf_environment, cfe};
use cf_traits::mocks::{
	ensure_origin_mock::NeverFailingOriginCheck,
	eth_environment_provider::MockEthEnvironmentProvider,
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

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		Environment: pallet_cf_environment,
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

impl pallet_cf_environment::Config for Test {
	type Event = Event;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type WeightInfo = ();
	type EthEnvironmentProvider = MockEthEnvironmentProvider;
}

pub const STAKE_MANAGER_ADDRESS: [u8; 20] = [0u8; 20];
pub const KEY_MANAGER_ADDRESS: [u8; 20] = [1u8; 20];
pub const VAULT_ADDRESS: [u8; 20] = [2u8; 20];
pub const ETH_CHAIN_ID: u64 = 1;

pub const CFE_SETTINGS: cfe::CfeSettings = cfe::CfeSettings {
	eth_block_safety_margin: 1,
	max_ceremony_stage_duration: 1,
	eth_priority_fee_percentile: 50,
};

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		environment: EnvironmentConfig {
			stake_manager_address: STAKE_MANAGER_ADDRESS,
			key_manager_address: KEY_MANAGER_ADDRESS,
			ethereum_chain_id: ETH_CHAIN_ID,
			eth_vault_address: VAULT_ADDRESS,
			cfe_settings: CFE_SETTINGS,
			flip_token_address: [0u8; 20],
			eth_usdc_address: [0x2; 20],
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
