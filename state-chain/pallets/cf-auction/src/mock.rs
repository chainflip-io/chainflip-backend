use super::*;
use crate as pallet_cf_auction;
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
use frame_support::{parameter_types, construct_runtime,};
use frame_support::traits::ValidatorRegistration;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

type Amount = u64;
type ValidatorId = u64;

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Auction: pallet_cf_auction::{Module, Storage},
		// Session: pallet_session::{Module, Call, Storage, Event, Config<T>},
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
impl Config for Test {
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type BidderProvider = TestBidderProvider;
	type Registrar = Self;
}

impl ValidatorRegistration<ValidatorId> for Test {
	fn is_registered(id: &ValidatorId) -> bool {
		true
	}
}

pub struct TestBidderProvider;

impl<ValidatorId, Amount> BidderProvider<ValidatorId, Amount> for TestBidderProvider {
	fn get_bidders() -> Vec<(ValidatorId, Amount)> {
		vec![]
	}
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	frame_system::GenesisConfig::default().assimilate_storage::<Test>(&mut t).unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
