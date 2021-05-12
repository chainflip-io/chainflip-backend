use crate::pallet as pallet_cf_core;
use sp_core::{H256, sr25519::Public};
use frame_support::parameter_types;
use sp_runtime::{Permill, testing::Header, traits::{BlakeTwo256, IdentityLookup}};
use frame_system as system;

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
		ChainflipCore: pallet_cf_core::{Module, Call, Storage, Event<T>},
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
	type AccountId = u64;
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

impl pallet_cf_core::Config for Test {
	type Event = Event;

	type Amount = ();

	type AutoSwap = ();

	// TODO: Implement PerThing for actual Bips instead of using Permill.
	type Bips = Permill;

	type BlockHash = ();

	type BlockNumber = ();

	type Chain = ();

	type Crypto = Public;

	type LiquidityPubKey = Public;

	type OutputAddress = ();

	type OutputId = ();

	type QuoteId = ();

	type SlashData = ();

	type SlashReason = ();

	type Ticker = ();

	type TxHash = ();

	type EthereumPubKey = Public;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
}
