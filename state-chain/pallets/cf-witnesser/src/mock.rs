use crate::{self as pallet_cf_witness, WitnessDataExtraction};
use cf_traits::mocks::{self, epoch_info::MockEpochInfo};
use frame_support::parameter_types;
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};

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
		System: frame_system,
		Witnesser: pallet_cf_witness,
		Dummy: dummy,
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

impl pallet_cf_witness::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type AccountRoleRegistry = ();
	type RuntimeCall = RuntimeCall;
	type ValidatorId = AccountId;
	type EpochInfo = mocks::epoch_info::Mock;
	type Amount = u64;
	type WeightInfo = ();
}

impl dummy::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type EnsureWitnessed = pallet_cf_witness::EnsureWitnessed;
}

impl WitnessDataExtraction for RuntimeCall {
	fn extract(&mut self) -> Option<Vec<u8>> {
		None
	}

	fn combine_and_inject(&mut self, _data: &mut [Vec<u8>]) {
		// Do nothing
	}
}

pub const ALISSA: <Test as frame_system::Config>::AccountId = 1u64;
pub const BOBSON: <Test as frame_system::Config>::AccountId = 2u64;
pub const CHARLEMAGNE: <Test as frame_system::Config>::AccountId = 3u64;
pub const DEIRDRE: <Test as frame_system::Config>::AccountId = 4u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		system::GenesisConfig::default().build_storage::<Test>().unwrap().into();

	const GENESIS_AUTHORITIES: [u64; 3] = [ALISSA, BOBSON, CHARLEMAGNE];

	ext.execute_with(|| {
		// This is required to log events.
		System::set_block_number(1);
		MockEpochInfo::next_epoch(GENESIS_AUTHORITIES.to_vec());
	});

	ext
}
