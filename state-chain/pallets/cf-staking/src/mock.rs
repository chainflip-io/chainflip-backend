use std::time::Duration;

use crate as pallet_cf_staking;
use frame_support::parameter_types;
use pallet_cf_flip;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

use cf_traits::mocks::epoch_info;
pub(super) mod ensure_witnessed;
pub(super) mod time_source;
pub(super) mod witnesser;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Flip: pallet_cf_flip::{Module, Call, Config<T>, Storage, Event<T>},
		Staking: pallet_cf_staking::{Module, Call, Config<T>, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
	pub const MinClaimTTL: Duration = Duration::from_millis(100);
	pub const ClaimTTL: Duration = Duration::from_millis(1000);
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

impl pallet_cf_staking::Config for Test {
	type Event = Event;
	type Call = Call;
	type Nonce = u64;
	type EnsureWitnessed = ensure_witnessed::Mock;
	type Witnesser = witnesser::Mock;
	type EpochInfo = epoch_info::Mock;
	type TimeSource = time_source::Mock;
	type MinClaimTTL = MinClaimTTL;
	type ClaimTTL = ClaimTTL;
	type Balance = u128;
	type Flip = Flip;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_flip: Some(FlipConfig {
			total_issuance: 1_000,
		}),
		pallet_cf_staking: Some(StakingConfig {
			genesis_stakers: vec![],
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
