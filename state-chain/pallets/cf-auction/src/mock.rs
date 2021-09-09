use super::*;
use crate as pallet_cf_auction;
use cf_traits::mocks::vault_rotation::Mock as MockAuctionHandler;
use frame_support::traits::ValidatorRegistration;
use frame_support::{construct_runtime, parameter_types};
use sp_core::H256;
use sp_runtime::BuildStorage;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use std::cell::RefCell;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

type Amount = u64;
type AccountId = u64;

pub const LOW_BID: (AccountId, Amount) = (2, 2);
pub const JOE_BID: (AccountId, Amount) = (3, 100);
pub const MAX_BID: (AccountId, Amount) = (4, 101);
pub const INVALID_BID: (AccountId, Amount) = (1, 0);

pub const MIN_AUCTION_SIZE: u32 = 2;
pub const MAX_AUCTION_SIZE: u32 = 150;

thread_local! {
	// A set of bidders, we initialise this with the proposed genesis bidders
	pub static BIDDER_SET: RefCell<Vec<(AccountId, Amount)>> = RefCell::new(vec![
		INVALID_BID, LOW_BID, JOE_BID, MAX_BID
	]);
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
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
}

parameter_types! {
	pub const MinAuctionSize: u32 = 2;
}

impl Config for Test {
	type Event = Event;
	type Amount = Amount;
	type AccountId = AccountId;
	type BidderProvider = TestBidderProvider;
	type Registrar = Test;
	type AuctionIndex = u32;
	type MinAuctionSize = MinAuctionSize;
	type Handler = MockAuctionHandler;
}

impl ValidatorRegistration<AccountId> for Test {
	fn is_registered(_id: &AccountId) -> bool {
		true
	}
}

pub struct TestBidderProvider;

impl BidderProvider for TestBidderProvider {
	type AccountId = AccountId;
	type Amount = Amount;

	fn get_bidders() -> Vec<(Self::AccountId, Self::Amount)> {
		BIDDER_SET.with(|l| l.borrow().to_vec())
	}
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_auction: Some(AuctionPalletConfig {
			auction_size_range: (MIN_AUCTION_SIZE, MAX_AUCTION_SIZE),
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
