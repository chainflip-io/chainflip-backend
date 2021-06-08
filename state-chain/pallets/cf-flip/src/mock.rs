use crate::{self as pallet_cf_flip, TotalIssuance, OffchainFunds, OnchainFunds};
use cf_traits::StakeTransfer;
use frame_support::parameter_types;
use frame_system::{Account, AccountInfo};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Flip: pallet_cf_flip::{Module, Call, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl frame_system::Config for Test {
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

parameter_types! {
	pub const ExistentialDeposit: u128 = 10;
}

impl pallet_cf_flip::Config for Test {
	type Event = Event;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
}

// Build genesis storage according to the mock runtime.
pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <Test as frame_system::Config>::AccountId = 789u64;

pub fn check_balance_integrity() {
	assert_eq!(
		TotalIssuance::<Test>::get(),
		OffchainFunds::<Test>::get() + OnchainFunds::<Test>::get()
	);

	assert_eq!(
		pallet_cf_flip::Account::<Test>::iter_values().map(|account| account.total()).sum::<u128>(),
		OnchainFunds::<Test>::get()
	);
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities = frame_system::GenesisConfig::default()
		.build_storage::<Test>()
		.unwrap()
		.into();

	// Seed with three active accounts.
	ext.execute_with(|| {
		System::set_block_number(1);
		Account::<Test>::insert(ALICE, AccountInfo::default());
		Account::<Test>::insert(BOB, AccountInfo::default());
		Account::<Test>::insert(CHARLIE, AccountInfo::default());
		TotalIssuance::<Test>::set(1_000);
		OffchainFunds::<Test>::set(1_000);
		OnchainFunds::<Test>::set(0);

		<Flip as StakeTransfer>::credit_stake(&ALICE, 100);
		<Flip as StakeTransfer>::credit_stake(&BOB, 50);

		assert_eq!(OffchainFunds::<Test>::get(), 850);
		check_balance_integrity();
	});

	ext
}
