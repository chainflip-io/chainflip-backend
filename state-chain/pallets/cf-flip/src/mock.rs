use crate::{self as pallet_cf_flip, BurnFlipAccount};
use cf_traits::StakeTransfer;
use frame_support::{
	parameter_types,
	traits::{EnsureOrigin, HandleLifetime},
	weights::IdentityFee,
};
use frame_system::{ensure_root, RawOrigin};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Flip: pallet_cf_flip::{Module, Call, Config<T>, Storage, Event<T>},
		TransactionPayment: pallet_transaction_payment::{Module, Storage},
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
	type OnKilledAccount = BurnFlipAccount<Self>;
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
}

pub type FlipBalance = u128;

parameter_types! {
	pub const ExistentialDeposit: FlipBalance = 10;
}

pub struct MockEnsureGovernance;

impl EnsureOrigin<Origin> for MockEnsureGovernance {
	type Success = ();

	fn try_origin(_o: Origin) -> Result<Self::Success, Origin> {
		Ok(().into())
	}
}

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

impl pallet_cf_flip::Config for Test {
	type Event = Event;
	type Balance = FlipBalance;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = MockEnsureGovernance;
	type BlocksPerDay = BlocksPerDay;
}

parameter_types! {
	pub const TransactionByteFee: FlipBalance = 1;
}

impl pallet_transaction_payment::Config for Test {
	type OnChargeTransaction = pallet_cf_flip::FlipTransactionPayment<Self>;
	type TransactionByteFee = TransactionByteFee;
	type WeightToFee = IdentityFee<FlipBalance>;
	type FeeMultiplierUpdate = ();
}

// Build genesis storage according to the mock runtime.
pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <Test as frame_system::Config>::AccountId = 789u64;

pub fn check_balance_integrity() {
	let accounts_total = pallet_cf_flip::Account::<Test>::iter_values()
		.map(|account| account.total())
		.sum::<FlipBalance>();
	let reserves_total = pallet_cf_flip::Reserve::<Test>::iter_values().sum::<FlipBalance>();

	assert_eq!(accounts_total + reserves_total, Flip::onchain_funds());
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_flip: Some(FlipConfig {
			total_issuance: 1_000,
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);

		// Seed with two staked accounts.
		frame_system::Provider::<Test>::created(&ALICE).unwrap();
		frame_system::Provider::<Test>::created(&BOB).unwrap();
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));
		assert!(frame_system::Pallet::<Test>::account_exists(&BOB));
		assert!(!frame_system::Pallet::<Test>::account_exists(&CHARLIE));
		<Flip as StakeTransfer>::credit_stake(&ALICE, 100);
		<Flip as StakeTransfer>::credit_stake(&BOB, 50);

		assert_eq!(Flip::offchain_funds(), 850);
		check_balance_integrity();
	});

	ext
}
