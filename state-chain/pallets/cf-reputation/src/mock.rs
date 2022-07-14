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

type ValidatorId = u64;

thread_local! {
	pub static SLASHES: RefCell<Vec<(u64, u64)>> = RefCell::new(Default::default());
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		ReputationPallet: pallet_cf_reputation,
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
	type AccountId = ValidatorId;
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
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

// A heartbeat interval in blocks
pub const HEARTBEAT_BLOCK_INTERVAL: u64 = 150;
pub const REPUTATION_PER_HEARTBEAT: ReputationPoints = 10;

pub const ACCRUAL_RATIO: (i32, u64) = (REPUTATION_PER_HEARTBEAT, HEARTBEAT_BLOCK_INTERVAL);

pub const MAX_REPUTATION_POINT_ACCRUED: ReputationPoints = 25;

pub const MISSED_HEARTBEAT_PENALTY_POINTS: ReputationPoints = 2;
pub const GRANDPA_EQUIVOCATION_PENALTY_POINTS: ReputationPoints = 50;
pub const GRANDPA_SUSPENSION_DURATION: u64 = HEARTBEAT_BLOCK_INTERVAL * 10;

parameter_types! {
	pub const HeartbeatBlockInterval: u64 = HEARTBEAT_BLOCK_INTERVAL;
	pub const ReputationPointFloorAndCeiling: (i32, i32) = (-2880, 2880);
	pub const MaximumReputationPointAccrued: ReputationPoints = MAX_REPUTATION_POINT_ACCRUED;
}

// Mocking the `Slasher` trait
pub struct MockSlasher;

impl MockSlasher {
	pub fn slash_count(validator_id: ValidatorId) -> usize {
		SLASHES
			.with(|slashes| slashes.borrow().iter().filter(|(id, _)| *id == validator_id).count())
	}
}

impl Slashing for MockSlasher {
	type AccountId = ValidatorId;
	type BlockNumber = u64;

	fn slash(validator_id: &Self::AccountId, blocks_offline: Self::BlockNumber) {
		// Count those slashes
		SLASHES.with(|count| {
			count.borrow_mut().push((*validator_id, blocks_offline));
		});
	}
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 100u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 200u64;

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = ValidatorId;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

thread_local! {
	pub static NETWORK_STATE: RefCell<NetworkState<ValidatorId>> = RefCell::new(
		NetworkState {
			offline: vec![],
			online: vec![],
		}
	);
}

pub struct MockHeartbeat;
impl Heartbeat for MockHeartbeat {
	type ValidatorId = ValidatorId;
	type BlockNumber = u64;

	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) {
		NETWORK_STATE.with(|cell| *cell.borrow_mut() = network_state);
	}
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum AllOffences {
	MissedHeartbeat,
	NotLockingYourComputer,
	ForgettingYourYubiKey,
	UpsettingGrandpa,
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
	type Heartbeat = MockHeartbeat;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = MockSlasher;
	type WeightInfo = ();
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type MaximumAccruableReputation = MaximumReputationPointAccrued;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		reputation_pallet: ReputationPalletConfig {
			accrual_ratio: ACCRUAL_RATIO,
			penalties: vec![
				(AllOffences::MissedHeartbeat, (MISSED_HEARTBEAT_PENALTY_POINTS, 0)),
				(AllOffences::ForgettingYourYubiKey, (15, HEARTBEAT_BLOCK_INTERVAL)),
				(AllOffences::NotLockingYourComputer, (15, HEARTBEAT_BLOCK_INTERVAL)),
				(
					AllOffences::UpsettingGrandpa,
					(GRANDPA_EQUIVOCATION_PENALTY_POINTS, GRANDPA_SUSPENSION_DURATION),
				),
			],
			genesis_nodes: vec![ALICE],
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
		MockEpochInfo::next_epoch(vec![ALICE]);
	});

	ext
}
