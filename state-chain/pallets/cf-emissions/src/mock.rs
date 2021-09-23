use std::marker::PhantomData;

use crate as pallet_cf_emissions;
use frame_support::{
	parameter_types,
	traits::{EnsureOrigin, Imbalance},
};
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

use cf_traits::{mocks::epoch_info, RewardsDistribution};

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

pub struct MockEnsureGovernance;

impl EnsureOrigin<Origin> for MockEnsureGovernance {
	type Success = ();

	fn try_origin(_o: Origin) -> Result<Self::Success, Origin> {
		Ok(().into())
	}
}

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

impl pallet_cf_flip::Config for Test {
	type Event = Event;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = MockEnsureGovernance;
	type BlocksPerDay = BlocksPerDay;
}

pub const MINT_INTERVAL: u64 = 5;

parameter_types! {
	pub const MintInterval: u64 = MINT_INTERVAL;
}

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);
cf_traits::impl_mock_witnesser_for_account_and_call_types!(u64, Call);

pub struct MockRewardsDistribution<T>(PhantomData<T>);

impl RewardsDistribution for MockRewardsDistribution<Test> {
	type Balance = u128;
	type Surplus = pallet_cf_flip::Surplus<Test>;

	fn distribute(rewards: Self::Surplus) {
		let reward_amount = rewards.peek();
		let deposit = Flip::deposit_reserves(*b"RSVR", reward_amount);
		let _ = rewards.offset(deposit);
	}

	fn execution_weight() -> frame_support::dispatch::Weight {
		1
	}
}

impl pallet_cf_emissions::Config for Test {
	type Event = Event;
	type FlipBalance = u128;
	type Surplus = pallet_cf_flip::Surplus<Test>;
	type Issuance = pallet_cf_flip::FlipIssuance<Test>;
	type RewardsDistribution = MockRewardsDistribution<Self>;
	type MintInterval = MintInterval;
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
		}),
	};
	for v in validators {
		epoch_info::Mock::add_validator(v);
	}
	config.build_storage().unwrap().into()
}
