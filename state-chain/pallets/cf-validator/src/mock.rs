use super::*;
use crate as pallet_cf_validator;
use sp_core::{H256};
use sp_runtime::{
	Perbill,
	impl_opaque_keys,
	traits::{
		BlakeTwo256,
		IdentityLookup,
		ConvertInto,
	},
	testing::{
		Header,
		UintAuthorityId,
	},
};
use frame_support::{parameter_types, construct_runtime, traits::{OnInitialize, OnFinalize}};
use std::cell::RefCell;
use cf_traits::{BidderProvider, AuctionConfirmation};
use frame_support::traits::ValidatorRegistration;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub type Amount = u64;
pub type ValidatorId = u64;

impl WeightInfo for () {
	fn set_blocks_for_epoch() -> u64 { 0 as Weight }

	fn force_rotation() -> u64 { 0 as Weight }
}

thread_local! {
	pub static CANDIDATE_IDX: RefCell<u64> = RefCell::new(0);
	pub static CURRENT_VALIDATORS: RefCell<Vec<u64>> = RefCell::new(vec![]);
	pub static PHASE: RefCell<AuctionPhase<ValidatorId, Amount>> =  RefCell::new(AuctionPhase::WaitingForBids);
	pub static BIDDERS: RefCell<Vec<(u64, u64)>> = RefCell::new(vec![]);
	pub static WINNERS: RefCell<Vec<u64>> = RefCell::new(vec![]);
	pub static CONFIRM: RefCell<bool> = RefCell::new(false);
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		ValidatorManager: pallet_cf_validator::{Module, Call, Storage, Event<T>},
		Session: pallet_session::{Module, Call, Storage, Event, Config<T>},
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

impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub dummy: UintAuthorityId,
	}
}

parameter_types! {
	pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
}

impl pallet_session::Config for Test {
	type ShouldEndSession = ValidatorManager;
	type SessionManager = ValidatorManager;
	type SessionHandler = ValidatorManager;
	type ValidatorId = ValidatorId;
	type ValidatorIdOf = ConvertInto;
	type Keys = MockSessionKeys;
	type Event = Event;
	type DisabledValidatorsThreshold = DisabledValidatorsThreshold;
	type NextSessionRotation = ();
	type WeightInfo = ();
}

parameter_types! {
	pub const MinAuctionSize: u32 = 2;
}

impl pallet_cf_auction::Config for Test {
	type Event = Event;
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type BidderProvider = TestBidderProvider;
	type Registrar = Test;
	type AuctionIndex = u32;
	type MinAuctionSize = MinAuctionSize;
	type Confirmation = TestConfirmation;
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
		let idx = CANDIDATE_IDX.with(|idx| {
			let new_idx = *idx.borrow_mut() + 1;
			*idx.borrow_mut() = new_idx;
			new_idx
		});

		vec![(1 + idx, 1), (2 + idx, 2)]
	}
}

pub struct TestConfirmation;
impl AuctionConfirmation for TestConfirmation {
	fn confirmed() -> bool {
		CONFIRM.with(|l| *l.borrow())
	}
}

pub struct TestEpochTransitionHandler;

impl EpochTransitionHandler for TestEpochTransitionHandler {
	type ValidatorId = ValidatorId;
	fn on_new_epoch(new_validators: Vec<Self::ValidatorId>) {
		CURRENT_VALIDATORS.with(|l|
			*l.borrow_mut() = new_validators
		);
	}
}

parameter_types! {
	pub const MinEpoch: u64 = 1;
	pub const MinValidatorSetSize: u32 = 2;
}

pub(super) type EpochIndex = u32;

impl Config for Test {
	type Event = Event;
	type MinEpoch = MinEpoch;
	type EpochTransitionHandler = TestEpochTransitionHandler;
	type ValidatorWeightInfo = ();
	type EpochIndex = EpochIndex;
	type Amount = Amount;
	// Use the pallet's implementation
	type Auction = AuctionPallet;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	frame_system::GenesisConfig::default().assimilate_storage::<Test>(&mut t).unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn current_validators() -> Vec<u64> {
	CURRENT_VALIDATORS.with(|l| l.borrow().to_vec())
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		Session::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		Session::on_initialize(System::block_number());
	}
}
