use crate::{self as pallet_cf_account_roles, Config};
use cf_traits::{
	impl_mock_stake_transfer,
	mocks::{
		bid_info::MockBidInfo, ensure_origin_mock::NeverFailingOriginCheck,
		system_state_info::MockSystemStateInfo,
	},
	Chainflip,
};

use frame_support::traits::{ConstU16, ConstU64};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		MockAccountRoles: pallet_cf_account_roles,
	}
);

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
	type AccountId = u64;
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
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

impl_mock_stake_transfer!(u64, u128);

impl Config for Test {
	type Event = Event;
	type MinBidInfo = MockBidInfo;
	type StakeManager = MockStakeHandler;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
}
