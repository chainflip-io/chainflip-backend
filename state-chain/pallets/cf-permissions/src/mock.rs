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

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Permissions: pallet_cf_permissions::{Module, Call, Storage, Event<T>},
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

impl Config for Test {
	type Event = Event;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	frame_system::GenesisConfig::default().assimilate_storage::<Test>(&mut t).unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		//Session::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		//Session::on_initialize(System::block_number());
	}
}
