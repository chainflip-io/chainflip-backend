use std::time::Duration;

use crate::{self as pallet_cf_staking, Config};
use app_crypto::ecdsa::Public;
use sp_core::H256;
use frame_support::{parameter_types};
use sp_runtime::{app_crypto, testing::Header, traits::{BlakeTwo256, IdentityLookup}};
use frame_system::{Account, AccountInfo};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

pub(super) mod epoch_info;
pub(super) mod witnesser;
pub(super) mod ensure_witnessed;
pub(super) mod time_source;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		StakeManager: pallet_cf_staking::{Module, Call, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
	pub const MinClaimTTL: Duration = Duration::from_millis(100);
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

impl Config for Test {
	type Event = Event;
	type Call = Call;
	type TokenAmount = u128;
	type EthereumAddress = [u8; 20];
	type Nonce = u32;
	type EthereumCrypto = Public;
	type EnsureWitnessed = ensure_witnessed::Mock;
	type Witnesser = witnesser::Mock;
	type EpochInfo = epoch_info::Mock;
	type TimeSource = time_source::Mock;
	type MinClaimTTL = MinClaimTTL;
} 

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into();

	// Seed with two active accounts.
	ext.execute_with(|| {
		System::set_block_number(1);
		Account::<Test>::insert(ALICE, AccountInfo::default());
		Account::<Test>::insert(BOB, AccountInfo::default());
	});

	ext
}
