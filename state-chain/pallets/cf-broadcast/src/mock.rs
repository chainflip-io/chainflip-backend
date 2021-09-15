use crate::{
	self as pallet_cf_broadcast, BaseConfig, BroadcastConfig, BroadcastFailure, SignerNomination,
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
		TransactionBroadcast: pallet_cf_broadcast::<Instance0>::{Module, Call, Storage, Event<T>},
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

pub struct MockNonceProvider;

impl cf_traits::NonceProvider for MockNonceProvider {
	fn next_nonce(identifier: cf_traits::NonceIdentifier) -> cf_traits::Nonce {
		1
	}
}

impl BaseConfig for Test {
	type KeyId = u64;
	type ValidatorId = u64;
	type ChainId = u64;
	type NonceProvider = MockNonceProvider;
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
	ThresholdSigReceived(Vec<u8>),
	Broadcasting,
	Complete,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct MockUnsignedTx;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct MockSignedTx;

impl BroadcastConfig<Test> for MockBroadcast {
	type Payload = Vec<u8>;
	type Signature = Vec<u8>;
	type UnsignedTransaction = MockUnsignedTx;
	type SignedTransaction = MockSignedTx;
	type TransactionHash = Vec<u8>;
	type Error = ();

	fn construct_signing_payload(&self) -> Result<Self::Payload, Self::Error> {
		assert_eq!(*self, MockBroadcast::New);
		Ok(b"payload".to_vec())
	}

	fn construct_unsigned_transaction(&self) -> Result<Self::UnsignedTransaction, Self::Error> {
		Ok(MockUnsignedTx)
	}

	fn on_broadcast_success(&mut self, transaction_hash: &Self::TransactionHash) {
		assert_eq!(transaction_hash, b"0x-tx-hash");
		*self = MockBroadcast::Complete;
	}

	fn on_broadcast_failure(&mut self, failure: &BroadcastFailure<u64>) {
		todo!()
	}

	fn add_threshold_signature(&mut self, sig: &Self::Signature) {
		assert_eq!(sig, b"signed-by-cfe");
		*self = MockBroadcast::ThresholdSigReceived(sig.clone());
	}

	fn verify_tx(
		&self,
		signer: &<Test as BaseConfig>::ValidatorId,
		_signed_tx: &Self::SignedTransaction,
	) -> Result<(), Self::Error> {
		assert_eq!(*signer, RANDOM_NOMINEE);
		Ok(())
	}
}

impl pallet_cf_broadcast::Config<Instance0> for Test {
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
