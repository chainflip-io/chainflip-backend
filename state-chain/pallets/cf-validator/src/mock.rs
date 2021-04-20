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

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

thread_local! {
	pub static SESSION_CHANGED: RefCell<bool> = RefCell::new(false);
	pub static CURRENT_VALIDATORS: RefCell<Vec<u64>> = RefCell::new(vec![]);
	pub static NEXT_VALIDATORS: RefCell<Vec<u64>> = RefCell::new(vec![]);
}

pub struct TestValidatorHandler;

impl ValidatorHandler<u64> for TestValidatorHandler {
	fn on_new_session(
		changed: bool,
		current_validators: Vec<u64>,
		next_validators: Vec<u64>,
	) {
		SESSION_CHANGED.with(|l| *l.borrow_mut() = changed);

		CURRENT_VALIDATORS.with(|l|
			*l.borrow_mut() = current_validators
		);

		NEXT_VALIDATORS.with(|l|
			*l.borrow_mut() = next_validators
		);
	}
	fn on_before_session_ending() {
		// Nothing for the moment
	}
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
	type ValidatorId = u64;
	type ValidatorIdOf = ConvertInto;
	type Keys = MockSessionKeys;
	type Event = Event;
	type DisabledValidatorsThreshold = DisabledValidatorsThreshold;
	type NextSessionRotation = ();
	type WeightInfo = ();
}

pub struct TestCandidateProvider;

impl CandidateProvider<u64, u64> for TestCandidateProvider {
	fn get_candidates(index: SessionIndex) -> Vec<(u64, u64)> {
		vec![(index as u64, 1), (index as u64, 2), (index as u64, 3)]
	}
}
parameter_types! {
	pub const MinEpoch: u64 = 1;
	pub const MinValidatorSetSize: u64 = 2;
}

impl Config for Test {
	type Event = Event;
	type MinEpoch = MinEpoch;
	type MinValidatorSetSize = MinValidatorSetSize;
	type ValidatorId = u64;
	type Stake = u64;
	type CandidateProvider = TestCandidateProvider;
	type ValidatorHandler = TestValidatorHandler;
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

pub fn next_validators() -> Vec<u64> {
	NEXT_VALIDATORS.with(|l| l.borrow().to_vec())
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		Session::on_finalize(System::block_number());
		System::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		System::on_initialize(System::block_number());
		Session::on_initialize(System::block_number());
	}
}
