use super::*;
use crate as pallet_cf_reputation;
use frame_support::{construct_runtime, parameter_types};
use sp_core::H256;
use sp_runtime::BuildStorage;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use sp_std::cell::RefCell;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

use cf_traits::mocks::epoch_info;
use cf_traits::mocks::epoch_info::Mock;
use cf_traits::{EmergencyRotation, Slashing};

thread_local! {
	pub static SLASH_COUNT: RefCell<u64> = RefCell::new(0);
	pub static EMERGENCY_ROTATION_REQUESTED: RefCell<bool> = RefCell::new(false);
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		ReputationPallet: pallet_cf_reputation::{Module, Call, Storage, Event<T>, Config<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

impl frame_system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
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
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
}

pub type ValidatorId = <Test as frame_system::Config>::AccountId;
// A heartbeat interval in blocks
pub const HEARTBEAT_BLOCK_INTERVAL: u64 = 150;
// Number of blocks being offline before you lose one point
pub const POINTS_PER_BLOCK_PENALTY: ReputationPenalty<u64> = ReputationPenalty {
	points: 1,
	blocks: 10,
};
// Number of blocks to be online to accrue a point
pub const ACCRUAL_BLOCKS: u64 = 2500;
// Number of accrual points
pub const ACCRUAL_POINTS: i32 = 1;

parameter_types! {
	pub const HeartbeatBlockInterval: u64 = HEARTBEAT_BLOCK_INTERVAL;
	pub const ReputationPointPenalty: ReputationPenalty<u64> = POINTS_PER_BLOCK_PENALTY;
	pub const ReputationPointFloorAndCeiling: (i32, i32) = (-2880, 2880);
	pub const EmergencyRotationPercentageTrigger: u8 = 80;
}

// Mocking the `Slasher` trait
pub struct MockSlasher;
impl Slashing for MockSlasher {
	type AccountId = u64;
	type BlockNumber = u64;

	fn slash(_validator_id: &Self::AccountId, _blocks_offline: Self::BlockNumber) -> Weight {
		// Count those slashes
		SLASH_COUNT.with(|count| {
			let mut c = count.borrow_mut();
			*c = *c + 1
		});
		0
	}
}

pub struct MockEmergencyRotation;
impl EmergencyRotation for MockEmergencyRotation {
	fn request_emergency_rotation() {
		EMERGENCY_ROTATION_REQUESTED.with(|requested| {
			*requested.borrow_mut() = true;
		});
	}
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 100u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 200u64;
pub const CHARLIE: <Test as frame_system::Config>::AccountId = 300u64;
pub const DAVE: <Test as frame_system::Config>::AccountId = 400u64;
pub const ERIN: <Test as frame_system::Config>::AccountId = 500u64;

impl Config for Test {
	type Event = Event;
	type ValidatorId = ValidatorId;
	type Amount = u128;
	type HeartbeatBlockInterval = HeartbeatBlockInterval;
	type ReputationPointPenalty = ReputationPointPenalty;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = MockSlasher;
	type EpochInfo = epoch_info::Mock;
	type EmergencyRotation = MockEmergencyRotation;
	type EmergencyRotationPercentageTrigger = EmergencyRotationPercentageTrigger;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_reputation: Some(ReputationPalletConfig {
			accrual_ratio: (ACCRUAL_POINTS, ACCRUAL_BLOCKS),
		}),
	};

	// We only expect Alice to be a validator at the moment
	Mock::add_validator(ALICE);
	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		ReputationPallet::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		ReputationPallet::on_initialize(System::block_number());
	}
}
