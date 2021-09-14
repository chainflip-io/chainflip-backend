use super::*;
use crate as pallet_cf_auction;
use cf_traits::mocks::vault_rotation::Mock as MockAuctionHandler;
use cf_traits::ChainflipAccountData;
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

pub const MIN_VALIDATOR_SIZE: u32 = 2;
pub const MAX_VALIDATOR_SIZE: u32 = 3;

thread_local! {
	// A set of bidders, we initialise this with the proposed genesis bidders
	pub static BIDDER_SET: RefCell<Vec<(ValidatorId, Amount)>> = RefCell::new(vec![]);
	pub static CHAINFLIP_ACCOUNTS: RefCell<HashMap<u64, ChainflipAccountData>> = RefCell::new(HashMap::new());
}

// Create a set of descending bids, including an invalid bid of amount 0
pub fn generate_bids(number_of_bids: u64) {
	BIDDER_SET.with(|cell| {
		for bid_number in (0..number_of_bids).rev() {
			(*(cell.borrow_mut())).push((bid_number + 1, bid_number * 100));
		}
	});
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		AuctionPallet: pallet_cf_auction::{Module, Call, Storage, Event<T>, Config},
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
	pub const MinValidators: u32 = 2;
}

impl Config for Test {
	type Event = Event;
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type BidderProvider = TestBidderProvider;
	type Registrar = Test;
	type AuctionIndex = u32;
	type MinValidators = MinValidators;
	type Handler = MockAuctionHandler;
	type ChainflipAccount = MockChainflipAccount;
	type AccountIdOf = ConvertInto;
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

#[test]
fn account_updates() {
	generate_bids(5);
	let data = MockChainflipAccount::get(&1);
	assert_eq!(data.state, ChainflipAccountState::Passive);
	MockChainflipAccount::update_state(&1, ChainflipAccountState::Backup);
	let data = MockChainflipAccount::get(&1);
	assert_eq!(data.state, ChainflipAccountState::Backup);
	MockChainflipAccount::update_state(&1, ChainflipAccountState::Validator);
	let data = MockChainflipAccount::get(&1);
	assert_eq!(data.state, ChainflipAccountState::Validator);
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
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_auction: Some(AuctionPalletConfig {
			validator_size_range: (MIN_VALIDATOR_SIZE, MAX_VALIDATOR_SIZE),
		}),
	};

	generate_bids(9);

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
