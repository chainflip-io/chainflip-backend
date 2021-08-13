use crate::{self as pallet_cf_governance};
use cf_traits::mocks::time_source;
use frame_support::parameter_types;
use frame_system as system;
use sp_core::H256;
use sp_runtime::BuildStorage;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

cf_traits::impl_mock_ensure_governance_for_origin!(Origin);

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Governance: pallet_cf_governance::{Module, Call, Storage, Event<T>, Config<T>, Origin},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
}

impl pallet_cf_governance::Config for Test {
	type Origin = Origin;
	type Call = Call;
	type Event = Event;
	type TimeSource = time_source::Mock;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLES: <Test as frame_system::Config>::AccountId = 789u64;
pub const EVE: <Test as frame_system::Config>::AccountId = 987u64;
pub const PETER: <Test as frame_system::Config>::AccountId = 988u64;
pub const MAX: <Test as frame_system::Config>::AccountId = 989u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_governance: Some(GovernanceConfig {
			members: vec![ALICE, BOB, CHARLES],
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		// This is required to log events.
		System::set_block_number(1);
	});

	ext
}
