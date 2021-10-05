use crate::{self as pallet_cf_broadcast, BroadcastConfig, Instance0, SignerNomination};
use cf_traits::Chainflip;
use codec::{Decode, Encode};
use frame_support::parameter_types;
use frame_system;
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
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		DogeBroadcast: pallet_cf_broadcast::<Instance0>::{Module, Call, Storage, Event<T>},
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

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);
cf_traits::impl_mock_offline_conditions!(u64);

impl Chainflip for Test {
	type KeyId = u32;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = MockEnsureWitnessed;
}

pub struct MockNominator;
pub const RANDOM_NOMINEE: u64 = 0xc001d00d as u64;

impl SignerNomination for MockNominator {
	type SignerId = u64;

	fn nomination_with_seed(_seed: u64) -> Self::SignerId {
		RANDOM_NOMINEE
	}

	fn threshold_nomination_with_seed(_seed: u64) -> Vec<Self::SignerId> {
		vec![RANDOM_NOMINEE]
	}
}

// Doge
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct Doge;
impl cf_chains::Chain for Doge {}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct MockBroadcast;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct MockUnsignedTx;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum MockSignedTx {
	Valid,
	Invalid,
}

impl BroadcastConfig<Test> for MockBroadcast {
	type Chain = Doge;
	type UnsignedTransaction = MockUnsignedTx;
	type SignedTransaction = MockSignedTx;
	type TransactionHash = [u8; 4];

	fn verify_transaction(
		_signer: &<Test as Chainflip>::ValidatorId,
		_unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
	) -> Option<()> {
		match signed_tx {
			MockSignedTx::Valid => Some(()),
			MockSignedTx::Invalid => None,
		}
	}
}

pub const SIGNING_EXPIRY_BLOCKS: <Test as frame_system::Config>::BlockNumber = 2;
pub const BROADCAST_EXPIRY_BLOCKS: <Test as frame_system::Config>::BlockNumber = 4;

parameter_types! {
	pub const SigningTimeout: <Test as frame_system::Config>::BlockNumber = SIGNING_EXPIRY_BLOCKS;
	pub const TransmissionTimeout: <Test as frame_system::Config>::BlockNumber = BROADCAST_EXPIRY_BLOCKS;
}

impl pallet_cf_broadcast::Config<Instance0> for Test {
	type Event = Event;
	type TargetChain = Doge;
	type BroadcastConfig = MockBroadcast;
	type SignerNomination = MockNominator;
	type OfflineReporter = MockOfflineReporter;
	type SigningTimeout = SigningTimeout;
	type TransmissionTimeout = TransmissionTimeout;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities = frame_system::GenesisConfig::default()
		.build_storage::<Test>()
		.unwrap()
		.into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
