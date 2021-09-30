use super::*;
use crate as pallet_cf_auction;
use cf_traits::mocks::vault_rotation::{clear_confirmation, Mock as MockVaultRotator};
use cf_traits::{Bid, ChainflipAccountData};
use frame_support::traits::ValidatorRegistration;
use frame_support::{construct_runtime, parameter_types};
use sp_core::H256;
use sp_runtime::BuildStorage;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use std::cell::RefCell;
use std::collections::HashMap;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub type Amount = u64;
pub type ValidatorId = u64;

impl WeightInfo for () {
	fn set_auction_size_range() -> u64 {
		0 as Weight
	}
}

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
// offset the ids to create unique bidder groups
pub fn generate_bids(number_of_bids: u32, group: u32) {
	BIDDER_SET.with(|cell| {
		let mut cell = cell.borrow_mut();
		(*cell).clear();
		for bid_number in (1..=number_of_bids as u64).rev() {
			(*cell).push((bid_number * group, bid_number * 100));
		}
	});
}

pub fn set_bidders(bidders: Vec<(ValidatorId, Amount)>) {
	BIDDER_SET.with(|cell| {
		*cell.borrow_mut() = bidders;
	});
}
pub fn run_auction(number_of_bids: u32, group: u32) {
	generate_bids(number_of_bids, group);

	AuctionPallet::process()
		.and(AuctionPallet::process().and_then(|_| {
			clear_confirmation();
			AuctionPallet::process().and(AuctionPallet::process())
		}))
		.unwrap();

	assert_eq!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
}

pub fn last_event() -> mock::Event {
	frame_system::Pallet::<Test>::events()
		.pop()
		.expect("Event expected")
		.event
}

// pub fn expected_bidding() -> Vec<Bid<ValidatorId, Amount>> {
// 	MockBidderProvider::get_bidders()
// }

// The set we would expect
pub fn expected_validating_set() -> (Vec<ValidatorId>, Amount) {
	let mut bidders = MockBidderProvider::get_bidders();
	bidders.truncate(MAX_VALIDATOR_SIZE as usize);
	(
		bidders
			.iter()
			.map(|(validator_id, _)| *validator_id)
			.collect(),
		bidders.last().unwrap().1,
	)
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		AuctionPallet: pallet_cf_auction::{Module, Call, Storage, Event<T>, Config<T>},
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
	type AccountData = ChainflipAccountData;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
}

parameter_types! {
	pub const MinValidators: u32 = MIN_VALIDATOR_SIZE;
	pub const BackupValidatorRatio: u32 = BACKUP_VALIDATOR_RATIO;
	pub const PercentageOfBackupValidatorsInEmergency: u32 = 30;
}

pub struct MockEmergencyRotation;

impl EmergencyRotation for MockEmergencyRotation {
	fn request_emergency_rotation() {
		EMERGENCY_ROTATION.with(|cell| *cell.borrow_mut() = true);
	}

	fn emergency_rotation_in_progress() -> bool {
		EMERGENCY_ROTATION.with(|cell| *cell.borrow())
	}

	fn emergency_rotation_completed() {}
}

impl Config for Test {
	type Event = Event;
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type BidderProvider = MockBidderProvider;
	type Registrar = Test;
	type AuctionIndex = u32;
	type MinValidators = MinValidators;
	type Handler = MockVaultRotator;
	type ChainflipAccount = MockChainflipAccount;
	type Online = MockOnline;
	type ActiveToBackupValidatorRatio = BackupValidatorRatio;
	type WeightInfo = ();
	type EmergencyRotation = MockEmergencyRotation;
	type PercentageOfBackupValidatorsInEmergency = PercentageOfBackupValidatorsInEmergency;
}

pub struct MockChainflipAccount;

impl ChainflipAccount for MockChainflipAccount {
	type AccountId = u64;

	fn get(account_id: &Self::AccountId) -> ChainflipAccountData {
		CHAINFLIP_ACCOUNTS.with(|cell| *cell.borrow().get(account_id).unwrap())
	}

	fn update_state(account_id: &Self::AccountId, state: ChainflipAccountState) {
		CHAINFLIP_ACCOUNTS.with(|cell| {
			cell.borrow_mut()
				.insert(*account_id, ChainflipAccountData { state });
		})
	}
}

pub struct MockOnline;
impl Online for MockOnline {
	type ValidatorId = ValidatorId;

	fn is_online(_validator_id: &Self::ValidatorId) -> bool {
		true
	}
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
		frame_system: Default::default(),
		pallet_cf_auction: Some(AuctionPalletConfig {
			validator_size_range: (MIN_VALIDATOR_SIZE, MAX_VALIDATOR_SIZE),
			winners,
			minimum_active_bid,
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
