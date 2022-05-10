use super::*;
use crate as pallet_cf_reputation;
use frame_support::{construct_runtime, parameter_types};
use serde::{Deserialize, Serialize};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};
use sp_std::cell::RefCell;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

use cf_traits::{
	mocks::{
		ensure_origin_mock::NeverFailingOriginCheck, epoch_info::MockEpochInfo,
		system_state_info::MockSystemStateInfo,
	},
	Chainflip, Slashing,
};

thread_local! {
	pub static SLASH_COUNT: RefCell<u64> = RefCell::new(0);
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		ReputationPallet: pallet_cf_reputation::{Pallet, Call, Storage, Event<T>, Config<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
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
	type OnSetCode = ();
}

// A heartbeat interval in blocks
pub const HEARTBEAT_BLOCK_INTERVAL: u64 = 150;
pub const REPUTATION_PER_HEARTBEAT: ReputationPoints = 15;

pub const POINTS_PER_BLOCK_PENALTY: ReputationPenaltyRate<u64> =
	ReputationPenaltyRate { points: 1, per_blocks: 10 };

// Accrue one point for every 10 blocks online.
pub const ACCRUAL_RATE: (i32, u64) = (1, 10);

pub const MAX_REPUTATION_POINT_ACCRUED: ReputationPoints = 15;

pub const MISSED_HEARTBEAT_PENALTY_POINTS: ReputationPoints = 2;

parameter_types! {
	pub const HeartbeatBlockInterval: u64 = HEARTBEAT_BLOCK_INTERVAL;
	pub const ReputationPointPenalty: ReputationPenaltyRate<u64> = POINTS_PER_BLOCK_PENALTY;
	pub const ReputationPointFloorAndCeiling: (i32, i32) = (-2880, 2880);
	pub const MaximumReputationPointAccrued: ReputationPoints = MAX_REPUTATION_POINT_ACCRUED;
}

// Mocking the `Slasher` trait
pub struct MockSlasher;
impl Slashing for MockSlasher {
	type AccountId = u64;
	type BlockNumber = u64;

	fn slash(_validator_id: &Self::AccountId, _blocks_offline: Self::BlockNumber) {
		// Count those slashes
		SLASH_COUNT.with(|count| {
			let mut c = count.borrow_mut();
			*c += 1
		});
	}
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 100u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 200u64;

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum AllOffences {
	MissedHeartbeat,
	NotLockingYourComputer,
	ForgettingYourYubiKey,
}

impl From<PalletOffence> for AllOffences {
	fn from(o: PalletOffence) -> Self {
		match o {
			PalletOffence::MissedHeartbeat => AllOffences::MissedHeartbeat,
		}
	}
}

impl Config for Test {
	type Event = Event;
	type Offence = AllOffences;
	type HeartbeatBlockInterval = HeartbeatBlockInterval;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = MockSlasher;
	type WeightInfo = ();
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type MaximumReputationPointAccrued = MaximumReputationPointAccrued;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		reputation_pallet: ReputationPalletConfig {
			accrual_ratio: ACCRUAL_RATE,
			penalties: vec![
				(AllOffences::MissedHeartbeat, (MISSED_HEARTBEAT_PENALTY_POINTS, 0)),
				(AllOffences::ForgettingYourYubiKey, (15, HEARTBEAT_BLOCK_INTERVAL)),
				(AllOffences::NotLockingYourComputer, (15, HEARTBEAT_BLOCK_INTERVAL)),
			],
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
