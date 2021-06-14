use super::*;
use crate as pallet_cf_auction;
use frame_system::{RawOrigin, ensure_root};
use sp_core::{H256};
use sp_runtime::{
	traits::{
		BlakeTwo256,
		IdentityLookup,
	},
	testing::{
		Header,
	},
};
use frame_support::{parameter_types, construct_runtime};
use frame_support::traits::ValidatorRegistration;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

use std::cell::RefCell;

type Amount = u64;
type ValidatorId = u64;

thread_local! {
	pub static BIDDER_SET: RefCell<Vec<(ValidatorId, Amount)>> = RefCell::new(vec![]);
	pub static CONFIRM: RefCell<bool> = RefCell::new(false);
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		AuctionPallet: pallet_cf_auction::{Module, Call, Storage, Event<T>},
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

pub struct MockEnsureWitness;

impl EnsureOrigin<Origin> for MockEnsureWitness {
	type Success = ();

	fn try_origin(o: Origin) -> Result<Self::Success, Origin> {
		ensure_root(o).or(Err(RawOrigin::None.into()))
	}
}

pub struct WitnesserMock;

impl cf_traits::Witnesser for WitnesserMock {
	type AccountId = u64;
	type Call = Call;

	fn witness(_who: Self::AccountId, _call: Self::Call) -> DispatchResultWithPostInfo {
		// We don't intend to test this, it's just to keep the compiler happy.
		unimplemented!()
	}
}

parameter_types! {
	pub const MinAuctionSize: u32 = 2;
}

impl Config for Test {
	type Event = Event;
	type Call = Call;
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type BidderProvider = TestBidderProvider;
	type Registrar = Test;
	type AuctionIndex = u32;
	type MinAuctionSize = MinAuctionSize;
	type Confirmation = Test;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = WitnesserMock;
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

impl AuctionConfirmation for Test {
	fn confirmed() -> bool {
		CONFIRM.with(|l| *l.borrow())
	}
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	frame_system::GenesisConfig::default().assimilate_storage::<Test>(&mut t).unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
