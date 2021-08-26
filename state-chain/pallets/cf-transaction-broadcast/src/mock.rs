use crate::{
	self as pallet_cf_transaction_broadcast, BaseConfig, BroadcastContext, BroadcastFailure, SignerNomination,
};
use codec::{Decode, Encode};
use frame_support::instances::Instance0;
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
		TransactionBroadcast: pallet_cf_transaction_broadcast::<Instance0>::{Module, Call, Storage, Event<T>},
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

impl BaseConfig for Test {
	type KeyId = u64;
	type ValidatorId = u64;
	type ChainId = u64;
}

pub struct MockNominator;
pub const RANDOM_NOMINEE: u64 = 0xc001d00d as u64;

impl SignerNomination for MockNominator {
	type SignerId = u64;

	fn nomination_with_seed(_seed: u64) -> Self::SignerId {
		RANDOM_NOMINEE
	}

	fn threshold_nomination_with_seed(seed: u64) -> Vec<Self::SignerId> {
		vec![RANDOM_NOMINEE]
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum MockBroadcast {
	New,
	PayloadConstructed,
	ThresholdSigReceived(Vec<u8>),
	Complete,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct MockUnsignedTx;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct MockSignedTx;

impl BroadcastContext<Test> for MockBroadcast {
	type Payload = Vec<u8>;
	type Signature = Vec<u8>;
	type UnsignedTransaction = MockUnsignedTx;
	type SignedTransaction = MockSignedTx;
	type TransactionHash = Vec<u8>;

	fn construct_signing_payload(&mut self) -> Self::Payload {
		assert_eq!(*self, MockBroadcast::New);
		*self = MockBroadcast::PayloadConstructed;
		b"payload".to_vec()
	}

	fn construct_unsigned_transaction(
		&mut self,
		sig: &Self::Signature,
	) -> Self::UnsignedTransaction {
		assert_eq!(sig, b"signed-by-cfe");
		*self = MockBroadcast::ThresholdSigReceived(sig.clone());
		MockUnsignedTx
	}

	fn on_transaction_ready(&mut self, _signed_tx: &Self::SignedTransaction) {
		*self = MockBroadcast::Complete;
	}

	fn on_broadcast_success(&mut self, transaction_hash: &Self::TransactionHash) {
		todo!()
	}

	fn on_broadcast_failure( &mut self, failure: &BroadcastFailure<u64>) {
		todo!()
	}
}

impl pallet_cf_transaction_broadcast::Config<Instance0> for Test {
	type Event = Event;
	type EnsureWitnessed = MockEnsureWitnessed;
	type BroadcastContext = MockBroadcast;
	type SignerNomination = MockNominator;
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
