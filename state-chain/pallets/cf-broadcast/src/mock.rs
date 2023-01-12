use std::cell::RefCell;

use crate::{self as pallet_cf_broadcast, Instance1, PalletOffence};
use cf_chains::{
	eth::Ethereum,
	mocks::{MockApiCall, MockEthereum, MockTransactionBuilder},
	ChainCrypto,
};
use cf_traits::{
	mocks::{
		ensure_origin_mock::NeverFailingOriginCheck, epoch_info::MockEpochInfo,
		signer_nomination::MockNominator, system_state_info::MockSystemStateInfo,
		threshold_signer::MockThresholdSigner,
	},
	Chainflip, EpochKey, KeyState,
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
		System: frame_system,
		Broadcaster: pallet_cf_broadcast::<Instance1>,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

thread_local! {
	pub static VALIDKEY: std::cell::RefCell<bool> = RefCell::new(true);
}

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type RuntimeCall = RuntimeCall;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = cf_traits::mocks::epoch_info::MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

pub const BROADCAST_EXPIRY_BLOCKS: <Test as frame_system::Config>::BlockNumber = 4;

parameter_types! {
	pub const BroadcastTimeout: <Test as frame_system::Config>::BlockNumber = BROADCAST_EXPIRY_BLOCKS;
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<u64, PalletOffence>;

// Mock KeyProvider
pub const VALID_AGG_KEY: [u8; 4] = [0, 0, 0, 0];

pub const INVALID_AGG_KEY: [u8; 4] = [1, 1, 1, 1];

thread_local! {
	pub static SIGNATURE_REQUESTS: RefCell<Vec<<Ethereum as ChainCrypto>::Payload>> = RefCell::new(vec![]);
}

pub struct MockKeyProvider;

impl cf_traits::KeyProvider<MockEthereum> for MockKeyProvider {
	fn current_epoch_key() -> EpochKey<<MockEthereum as ChainCrypto>::AggKey> {
		EpochKey {
			key: if VALIDKEY.with(|cell| *cell.borrow()) { VALID_AGG_KEY } else { INVALID_AGG_KEY },
			epoch_index: Default::default(),
			key_state: KeyState::Active,
		}
	}
}

impl MockKeyProvider {
	pub fn set_valid(valid: bool) {
		VALIDKEY.with(|cell| *cell.borrow_mut() = valid);
	}
}

impl pallet_cf_broadcast::Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Offence = PalletOffence;
	type AccountRoleRegistry = ();
	type TargetChain = MockEthereum;
	type ApiCall = MockApiCall<MockEthereum>;
	type TransactionBuilder = MockTransactionBuilder<Self::TargetChain, Self::ApiCall>;
	type ThresholdSigner = MockThresholdSigner<MockEthereum, RuntimeCall>;
	type BroadcastSignerNomination = MockNominator;
	type OffenceReporter = MockOffenceReporter;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type BroadcastTimeout = BroadcastTimeout;
	type WeightInfo = ();
	type KeyProvider = MockKeyProvider;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
		MockEpochInfo::next_epoch(vec![1, 2, 3]);
	});

	ext
}
