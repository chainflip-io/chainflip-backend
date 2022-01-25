use super::*;
use crate as pallet_cf_auction;
use cf_traits::{
	impl_mock_online,
	mocks::{
		chainflip_account::MockChainflipAccount,
		vault_rotation::{clear_confirmation, Mock as MockVaultRotator},
	},
	Bid, ChainflipAccountData, EmergencyRotation,
};
use frame_support::{construct_runtime, parameter_types, traits::ValidatorRegistration};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};
use std::{cell::RefCell, collections::HashMap};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub type Amount = u64;
pub type ValidatorId = u64;

pub const MIN_VALIDATOR_SIZE: u32 = 1;
pub const MAX_VALIDATOR_SIZE: u32 = 3;
pub const BACKUP_VALIDATOR_RATIO: u32 = 3;
pub const NUMBER_OF_BIDDERS: u32 = 9;
pub const BIDDER_GROUP_A: u32 = 1;
pub const BIDDER_GROUP_B: u32 = 2;

thread_local! {
	// A set of bidders, we initialise this with the proposed genesis bidders
	pub static BIDDER_SET: RefCell<Vec<(ValidatorId, Amount)>> = RefCell::new(vec![]);
	pub static CHAINFLIP_ACCOUNTS: RefCell<HashMap<u64, ChainflipAccountData>> = RefCell::new(HashMap::new());
	pub static EMERGENCY_ROTATION: RefCell<bool> = RefCell::new(false);
}

// Create a set of descending bids, including an invalid bid of amount 0
// offset the ids to create unique bidder groups.  By default all bidders are online.
pub fn generate_bids(number_of_bids: u32, group: u32) {
	BIDDER_SET.with(|cell| {
		let mut cell = cell.borrow_mut();
		(*cell).clear();
		for bid_number in (1..=number_of_bids as u64).rev() {
			let validator_id = bid_number * group as u64;
			MockOnline::set_online(&validator_id, true);
			(*cell).push((validator_id, bid_number * 100));
		}
	});
}

pub fn set_bidders(bidders: Vec<(ValidatorId, Amount)>) {
	BIDDER_SET.with(|cell| {
		*cell.borrow_mut() = bidders;
	});
}

pub fn run_auction() {
	AuctionPallet::process()
		.and_then(|_| {
			clear_confirmation();
			AuctionPallet::process()
		})
		.unwrap();

	assert_eq!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
}

pub fn last_event() -> mock::Event {
	frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
}

// The set we would expect
pub fn expected_validating_set() -> (Vec<ValidatorId>, Amount) {
	let mut bidders = MockBidderProvider::get_bidders();
	bidders.truncate(MAX_VALIDATOR_SIZE as usize);
	(bidders.iter().map(|(validator_id, _)| *validator_id).collect(), bidders.last().unwrap().1)
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		AuctionPallet: pallet_cf_auction::{Pallet, Call, Storage, Event<T>, Config<T>},
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
	type AccountData = ChainflipAccountData;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
}

parameter_types! {
	pub const MinValidators: u32 = MIN_VALIDATOR_SIZE;
	pub const BackupValidatorRatio: u32 = BACKUP_VALIDATOR_RATIO;
	pub const PercentageOfBackupValidatorsInEmergency: u32 = 30;
}

pub struct MockEmergencyRotation;

impl EmergencyRotation for MockEmergencyRotation {
	fn request_emergency_rotation() -> Weight {
		EMERGENCY_ROTATION.with(|cell| *cell.borrow_mut() = true);
		0
	}

	fn emergency_rotation_in_progress() -> bool {
		EMERGENCY_ROTATION.with(|cell| *cell.borrow())
	}

	fn emergency_rotation_completed() {}
}

impl_mock_online!(ValidatorId);

pub struct MockPeerMapping;

impl HasPeerMapping for MockPeerMapping {
	type ValidatorId = ValidatorId;

	fn has_peer_mapping(_validator_id: &Self::ValidatorId) -> bool {
		true
	}
}

impl Config for Test {
	type Event = Event;
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type BidderProvider = MockBidderProvider;
	type Registrar = Test;
	type MinValidators = MinValidators;
	type Handler = MockVaultRotator;
	type ChainflipAccount = MockChainflipAccount;
	type Online = MockOnline;
	type PeerMapping = MockPeerMapping;
	type ActiveToBackupValidatorRatio = BackupValidatorRatio;
	type WeightInfo = ();
	type EmergencyRotation = MockEmergencyRotation;
	type PercentageOfBackupValidatorsInEmergency = PercentageOfBackupValidatorsInEmergency;
}

impl ValidatorRegistration<ValidatorId> for Test {
	fn is_registered(_id: &ValidatorId) -> bool {
		true
	}
}

pub struct MockBidderProvider;

impl BidderProvider for MockBidderProvider {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
		BIDDER_SET.with(|l| l.borrow().to_vec())
	}
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);

	let (winners, minimum_active_bid) = expected_validating_set();
	let config = GenesisConfig {
		system: Default::default(),
		auction_pallet: AuctionPalletConfig {
			validator_size_range: (MIN_VALIDATOR_SIZE, MAX_VALIDATOR_SIZE),
			winners,
			minimum_active_bid,
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
