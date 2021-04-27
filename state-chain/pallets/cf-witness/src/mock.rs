use crate::{self as pallet_cf_witness};
use sp_core::H256;
use frame_support::parameter_types;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup}, testing::Header,
};
use frame_system as system;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

pub mod dummy;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Witnesser: pallet_cf_witness::{Module, Call, Storage, Event<T>, Origin},
		Dummy: dummy::{Module, Call, Storage, Event<T>},
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
}

impl pallet_cf_witness::Config for Test {
	type Event = Event;
	type Origin = Origin;
	type Call = Call;

	type Epoch = u64;
	type ValidatorId = AccountId;
}

impl dummy::Config for Test {
    type Event = Event;
    type Call = Call;
    type EnsureWitnessed = pallet_cf_witness::EnsureWitnessed;
    type Witnesser = pallet_cf_witness::Pallet<Test>;
}

pub const ALISSA: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOBSON: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLEMAGNE: <Test as frame_system::Config>::AccountId = 789u64;
pub const DEIRDRE: <Test as frame_system::Config>::AccountId = 987u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext : sp_io::TestExternalities = system::GenesisConfig::default().build_storage::<Test>().unwrap().into();

	// Seed with two active validators and set the consensus threshold to two.
	ext.execute_with(|| {
		pallet_cf_witness::ValidatorIndex::<Test>::insert(0, ALISSA, 0);
		pallet_cf_witness::ValidatorIndex::<Test>::insert(0, BOBSON, 1);
		pallet_cf_witness::ValidatorIndex::<Test>::insert(0, CHARLEMAGNE, 2);
		pallet_cf_witness::NumValidators::<Test>::set(3);
		pallet_cf_witness::ConsensusThreshold::<Test>::set(2);
	});

	ext
}
