use std::cell::RefCell;

use crate::{
	self as pallet_cf_broadcast, AttemptCount, Instance1, PalletOffence, SignerNomination,
};
use cf_chains::mocks::{MockApiCall, MockEthereum, MockTransactionBuilder};
use cf_traits::{
	mocks::{
		ensure_origin_mock::NeverFailingOriginCheck, system_state_info::MockSystemStateInfo,
		threshold_signer::MockThresholdSigner,
	},
	Chainflip, EpochIndex,
};
use frame_support::parameter_types;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
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
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		MockBroadcast: pallet_cf_broadcast::<Instance1>::{Pallet, Call, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

thread_local! {
	pub static NOMINATION: std::cell::RefCell<Option<u64>>  = RefCell::new(Some(RANDOM_NOMINEE));
}

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
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
	type OnSetCode = ();
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessedByHistoricalActiveEpoch = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedByCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

pub struct MockNominator;
pub const RANDOM_NOMINEE: u64 = 0xc001d00d_u64;

impl SignerNomination for MockNominator {
	type SignerId = u64;

	fn nomination_with_seed<S>(_seed: S) -> Option<Self::SignerId> {
		NOMINATION.with(|cell| *cell.borrow())
	}

	fn threshold_nomination_with_seed<S>(
		_seed: S,
		_epoch_index: EpochIndex,
	) -> Option<Vec<Self::SignerId>> {
		Some(vec![RANDOM_NOMINEE])
	}
}

pub const SIGNING_EXPIRY_BLOCKS: <Test as frame_system::Config>::BlockNumber = 2;
pub const TRANSMISSION_EXPIRY_BLOCKS: <Test as frame_system::Config>::BlockNumber = 4;
pub const MAXIMUM_BROADCAST_ATTEMPTS: AttemptCount = 3;

parameter_types! {
	pub const SigningTimeout: <Test as frame_system::Config>::BlockNumber = SIGNING_EXPIRY_BLOCKS;
	pub const TransmissionTimeout: <Test as frame_system::Config>::BlockNumber = TRANSMISSION_EXPIRY_BLOCKS;
	pub const MaximumAttempts: AttemptCount = MAXIMUM_BROADCAST_ATTEMPTS;
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<u64, PalletOffence>;

impl pallet_cf_broadcast::Config<Instance1> for Test {
	type Event = Event;
	type Call = Call;
	type Offence = PalletOffence;
	type TargetChain = MockEthereum;
	type ApiCall = MockApiCall<MockEthereum>;
	type TransactionBuilder = MockTransactionBuilder<Self::TargetChain, Self::ApiCall>;
	type ThresholdSigner = MockThresholdSigner<MockEthereum, Call>;
	type SignerNomination = MockNominator;
	type OffenceReporter = MockOffenceReporter;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type SigningTimeout = SigningTimeout;
	type TransmissionTimeout = TransmissionTimeout;
	type WeightInfo = ();
	type MaximumAttempts = MaximumAttempts;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
