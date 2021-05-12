use std::{cell::RefCell, marker::PhantomData};

use crate::{self as pallet_cf_staking, Config};
use app_crypto::ecdsa::Public;
use sp_core::H256;
use frame_support::{dispatch::Dispatchable, parameter_types, traits::EnsureOrigin};
use sp_runtime::{app_crypto, testing::Header, traits::{BlakeTwo256, IdentityLookup}};
use frame_system as system;
use system::{Account, AccountInfo, RawOrigin, ensure_root};

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
		StakeManager: pallet_cf_staking::{Module, Call, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
	pub const UnsignedPriority: u32 = 100;
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

impl Config for Test {
	type Event = Event;
	type Call = Call;

	type StakedAmount = u128;

	type EthereumAddress = u64;

	type Nonce = u32;

	type EthereumCrypto = Public;

	type EnsureWitnessed = MockEnsureWitnessed;

	type Witnesser = MockWitnesser;
}

pub struct MockEnsureWitnessed;

impl EnsureOrigin<Origin> for MockEnsureWitnessed {
	type Success = ();

	fn try_origin(o: Origin) -> Result<Self::Success, Origin> {
		ensure_root(o).or(Err(RawOrigin::None.into()))
	}
}

pub struct MockWitnesser;

thread_local! {
	pub static WITNESS_THRESHOLD: RefCell<u32> = RefCell::new(0);
	pub static WITNESS_VOTES: RefCell<Vec<Call>> = RefCell::new(vec![]);
}

impl cf_traits::Witnesser for MockWitnesser {
	type AccountId = AccountId;
	type Call = Call;

	fn witness(_who: Self::AccountId, call: Self::Call) -> frame_support::dispatch::DispatchResultWithPostInfo {
		let count = WITNESS_VOTES.with(|votes| {
			let mut votes = votes.borrow_mut();
			votes.push(call.clone());
			votes.iter().filter(|vote| **vote == call.clone()).count()
		});

		let threshold = WITNESS_THRESHOLD.with(|t| t.borrow().clone());

		if count as u32 == threshold {
			Dispatchable::dispatch(call, Origin::root())
		} else {
			Ok(().into())
		}
	}
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities = system::GenesisConfig::default().build_storage::<Test>().unwrap().into();

	// Seed with two active accounts.
	ext.execute_with(|| {
		Account::<Test>::insert(ALICE, AccountInfo::default());
		Account::<Test>::insert(BOB, AccountInfo::default());
	});

	ext
}
