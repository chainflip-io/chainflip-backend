use crate as pallet_cf_rewards;
use cf_traits::{
	mocks::ensure_origin_mock::NeverFailingOriginCheck, RewardRollover, StakeTransfer,
};
use frame_support::{assert_ok, parameter_types, traits::EnsureOrigin};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Flip: pallet_cf_flip::{Module, Event<T>, Storage, Config<T>},
		FlipRewards: pallet_cf_rewards::{Module, Storage, Event<T>},
	}
);

pub type AccountId = u64;

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

parameter_types! {
	pub const ExistentialDeposit: u128 = 10;
}

cf_traits::impl_mock_stake_transfer!(u64, u128);

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

impl pallet_cf_flip::Config for Test {
	type Event = Event;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type BlocksPerDay = BlocksPerDay;
	type StakeHandler = MockStakeHandler;
	type WeightInfo = ();
}

impl pallet_cf_rewards::Config for Test {
	type Event = Event;
	type WeightInfoRewards = ();
}

pub fn check_balance_integrity() {
	let accounts_total = pallet_cf_flip::Account::<Test>::iter_values()
		.map(|account| account.total())
		.sum::<u128>();
	let reserves_total = pallet_cf_flip::Reserve::<Test>::iter_values().sum::<u128>();

	assert_eq!(accounts_total + reserves_total, Flip::onchain_funds());

	// Also check we enough reserves to honour our rewards payout.
	assert!(FlipRewards::sufficient_reserves());
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <Test as frame_system::Config>::AccountId = 789u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext(
	issuance: Option<u128>,
	accounts: Vec<(AccountId, u128)>,
) -> sp_io::TestExternalities {
	let total_issuance = issuance.unwrap_or(1_000u128);
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_flip: Some(FlipConfig { total_issuance }),
	};
	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();
	ext.execute_with(|| {
		let mut beneficiaries = vec![];
		for (acct, amt) in accounts {
			<Flip as StakeTransfer>::credit_stake(&acct, amt);
			beneficiaries.push(acct.clone());
		}
		// Rollover to initialize pallet state.
		assert_ok!(<FlipRewards as RewardRollover>::rollover(&beneficiaries));
	});
	ext
}
