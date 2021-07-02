use crate as pallet_cf_emissions;
use frame_support::parameter_types;
use frame_system as system;
use pallet_cf_flip;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

use cf_traits::mocks::epoch_info;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Flip: pallet_cf_flip::{Module, Call, Config<T>, Storage, Event<T>},
		Emissions: pallet_cf_emissions::{Module, Call, Config<T>, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
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
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 10;
}

impl pallet_cf_flip::Config for Test {
	type Event = Event;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
}

pub const MINT_FREQUENCY: u64 = 5;

parameter_types! {
	pub const MintFrequency: u64 = MINT_FREQUENCY;
}

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);
cf_traits::impl_mock_witnesser_for_account_and_call_types!(u64, Call);

impl pallet_cf_emissions::Config for Test {
	type Event = Event;
	type Call = Call;
	type FlipBalance = u128;
	type Emissions = Flip;
	type EnsureWitnessed = MockEnsureWitnessed;
	type Witnesser = MockWitnesser;
	type RewardsDistribution = pallet_cf_emissions::NaiveRewardsDistribution<Self>;
	type Validators = epoch_info::Mock;
	type MintFrequency = MintFrequency;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext(
	validators: Vec<u64>,
	issuance: Option<u128>,
	emissions: Option<u128>,
) -> sp_io::TestExternalities {
	let total_issuance = issuance.unwrap_or(1_000u128);
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_flip: Some(FlipConfig { total_issuance }),
		pallet_cf_emissions: Some(EmissionsConfig {
			emission_per_block: emissions.unwrap_or(total_issuance / 100),
			// 12 makes for nicer maths than the default which is 13
			eth_block_time: 12,
			native_block_time: 6,
		}),
	};
	for v in validators {
		epoch_info::Mock::add_validator(v);
	}
	config.build_storage().unwrap().into()
}
