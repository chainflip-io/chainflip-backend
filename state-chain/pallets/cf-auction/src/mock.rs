use super::*;
use crate as pallet_cf_auction;
use cf_traits::mocks::vault_rotation::{clear_confirmation, Mock as MockVaultRotation};
use cf_traits::{Bid, ChainflipAccountData};
use frame_support::traits::ValidatorRegistration;
use frame_support::{construct_runtime, parameter_types};
use sp_core::H256;
use sp_runtime::traits::ConvertInto;
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

thread_local! {
	// A set of bidders, we initialise this with the proposed genesis bidders
	pub static BIDDER_SET: RefCell<Vec<(ValidatorId, Amount)>> = RefCell::new(vec![]);
	pub static CHAINFLIP_ACCOUNTS: RefCell<HashMap<u64, ChainflipAccountData>> = RefCell::new(HashMap::new());
}

// Create a set of descending bids, including an invalid bid of amount 0
pub fn generate_bids(number_of_bids: u32) {
	BIDDER_SET.with(|cell| {
		let mut cell = cell.borrow_mut();
		(*cell).clear();
		for bid_number in (0..number_of_bids as u64).rev() {
			(*cell).push((bid_number + 1, bid_number * 100));
		}
	});
}

pub fn run_auction(number_of_bids: u32) {
	generate_bids(number_of_bids);

	let _ = AuctionPallet::process()
		.and(AuctionPallet::process().and_then(|_| {
			clear_confirmation();
			AuctionPallet::process()
		}))
		.unwrap();
}

pub fn last_event() -> mock::Event {
	frame_system::Pallet::<Test>::events()
		.pop()
		.expect("Event expected")
		.event
}

// The last is invalid as it has a bid of 0
pub fn expected_bidding() -> Vec<Bid<ValidatorId, Amount>> {
	let mut bidders = TestBidderProvider::get_bidders();
	bidders.pop();
	bidders
}

// The set we would expect
pub fn expected_validating_set() -> (Vec<ValidatorId>, Amount) {
	let mut bidders = TestBidderProvider::get_bidders();
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
}

impl Config for Test {
	type Event = Event;
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type BidderProvider = TestBidderProvider;
	type Registrar = Test;
	type AuctionIndex = u32;
	type MinValidators = MinValidators;
	type Handler = MockVaultRotation;
	type ChainflipAccount = MockChainflipAccount;
	type AccountIdOf = ConvertInto;
	type Online = MockOnline;
	type BackupValidatorRatio = BackupValidatorRatio;
	type WeightInfo = ();
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

pub struct TestBidderProvider;

impl BidderProvider for TestBidderProvider {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn get_bidders() -> Vec<(Self::ValidatorId, Self::Amount)> {
		BIDDER_SET.with(|l| l.borrow().to_vec())
	}
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	generate_bids(NUMBER_OF_BIDDERS);

	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_auction: Some(AuctionPalletConfig {
			validator_size_range: (MIN_VALIDATOR_SIZE, MAX_VALIDATOR_SIZE),
			winners: TestBidderProvider::get_bidders()
				.iter()
				.map(|(validator_id, _)| validator_id.clone())
				.collect(),
			minimum_active_bid: (NUMBER_OF_BIDDERS as u64 - 1) * 100,
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
