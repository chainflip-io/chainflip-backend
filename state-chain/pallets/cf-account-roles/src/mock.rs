use crate::{self as pallet_cf_account_roles, Config};
use cf_traits::{
	impl_mock_stake_transfer, impl_mock_waived_fees,
	mocks::{
		bid_info::MockBidInfo, ensure_origin_mock::NeverFailingOriginCheck,
		system_state_info::MockSystemStateInfo,
	},
	Chainflip, StakingInfo,
};

use cf_traits::WaivedFees;

use frame_support::{
	parameter_types,
	traits::{ConstU16, ConstU64},
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
type Balance = u128;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		MockAccountRoles: pallet_cf_account_roles,
		Flip: pallet_cf_flip,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
	pub const VotingPeriod: BlockNumberFor<Test> = 10;
	pub const ProposalFee: u128 = 100;
	pub const EnactmentDelay: BlockNumberFor<Test> = 20;
	pub const BlocksPerDay: u64 = 14400;
	pub const ExistentialDeposit: u128 = 10;
}

// Implement mock for RestrictionHandler
impl_mock_waived_fees!(AccountId, Call);
impl_mock_stake_transfer!(AccountId, u128);

impl pallet_cf_flip::Config for Test {
	type Event = Event;
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type BlocksPerDay = BlocksPerDay;
	type StakeHandler = MockStakeHandler;
	type WeightInfo = ();
	type WaivedFees = WaivedFeesMock;
}

impl frame_system::Config for Test {
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
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = MockAccountRoles;
	type OnKilledAccount = MockAccountRoles;
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<42>;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = AccountId;
	type Amount = Balance;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

pub struct MockStakingInfo;

impl StakingInfo for MockStakingInfo {
	type AccountId = AccountId;

	type Balance = Balance;

	fn total_stake_of(_: &Self::AccountId) -> Self::Balance {
		todo!()
	}

	fn total_onchain_stake() -> Self::Balance {
		todo!()
	}
}

impl Config for Test {
	type Event = Event;
	type BidInfo = MockBidInfo;
	type StakeInfo = MockStakingInfo;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
}
