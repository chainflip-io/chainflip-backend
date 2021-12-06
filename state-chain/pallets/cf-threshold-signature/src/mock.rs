use std::{collections::BTreeSet, iter::FromIterator};

use crate::{self as pallet_cf_threshold_signature, EnsureThresholdSigned};
use cf_chains::{eth, ChainCrypto};
use cf_traits::{Chainflip, SigningContext};
use codec::{Decode, Encode};
use frame_support::{
	instances::Instance1,
	parameter_types,
	traits::{EnsureOrigin, UnfilteredDispatchable},
};
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
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		DogeThresholdSigner: pallet_cf_threshold_signature::<Instance1>::{Pallet, Origin<T>, Call, Storage, Event<T>, ValidateUnsigned},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
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

use cf_traits::mocks::{ensure_origin_mock::NeverFailingOriginCheck, epoch_info::MockEpochInfo};

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EpochInfo = MockEpochInfo;
}

// Mock SignerNomination

thread_local! {
	pub static THRESHOLD_NOMINEES: std::cell::RefCell<Vec<u64>> = Default::default();
}

pub struct MockNominator;

impl MockNominator {
	pub fn set_nominees(nominees: Vec<u64>) {
		THRESHOLD_NOMINEES.with(|cell| *cell.borrow_mut() = nominees)
	}

	pub fn get_nominees() -> Vec<u64> {
		THRESHOLD_NOMINEES.with(|cell| cell.borrow().clone())
	}
}

impl cf_traits::SignerNomination for MockNominator {
	type SignerId = u64;

	fn nomination_with_seed(_seed: u64) -> Option<Self::SignerId> {
		unimplemented!("Single signer nomination not needed for these tests.")
	}

	fn threshold_nomination_with_seed(_seed: u64) -> Vec<Self::SignerId> {
		Self::get_nominees()
	}
}

// Mock Callback

thread_local! {
	pub static CALL_DISPATCHED: std::cell::RefCell<bool> = Default::default();
}

pub struct MockCallback<C: ChainCrypto>(pub String, pub C::ThresholdSignature);

impl MockCallback<Doge> {
	pub fn call() {
		CALL_DISPATCHED.with(|cell| *(cell.borrow_mut()) = true);
	}

	pub fn has_executed() -> bool {
		CALL_DISPATCHED.with(|cell| *cell.borrow())
	}

	pub fn reset() {
		CALL_DISPATCHED.with(|cell| *(cell.borrow_mut()) = false);
	}
}

impl UnfilteredDispatchable for MockCallback<Doge> {
	type Origin = Origin;

	fn dispatch_bypass_filter(
		self,
		origin: Self::Origin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		EnsureThresholdSigned::<Test, Instance1>::ensure_origin(origin)?;
		Self::call();
		Ok(().into())
	}
}

// Mock KeyProvider
pub const MOCK_KEY_ID: &'static [u8] = b"d06e";

pub struct MockKeyProvider;

impl cf_traits::KeyProvider<Doge> for MockKeyProvider {
	type KeyId = Vec<u8>;

	fn current_key_id() -> Self::KeyId {
		MOCK_KEY_ID.to_vec()
	}

	fn current_key() -> <Doge as ChainCrypto>::AggKey {
		eth::AggKey::from_pubkey_compressed(hex_literal::hex!(
			"0331b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae"
		))
	}
}

// Mock OfflineReporter
cf_traits::impl_mock_offline_conditions!(u64);

// Mock SigningContext

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct Doge;
impl cf_chains::Chain for Doge {
	const CHAIN_ID: cf_chains::ChainId = cf_chains::ChainId::Ethereum;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum DogeSig {
	Valid,
	Invalid,
}

impl ChainCrypto for Doge {
	type AggKey = eth::AggKey;
	type Payload = String;
	type ThresholdSignature = DogeSig;

	fn verify_threshold_signature(
		_agg_key: &Self::AggKey,
		_payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		*signature == DogeSig::Valid
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct DogeThresholdSignerContext {
	pub message: String,
}

pub const VALID_SIGNATURE: DogeSig = DogeSig::Valid;
pub const INVALID_SIGNATURE: DogeSig = DogeSig::Invalid;

impl SigningContext<Test> for DogeThresholdSignerContext {
	type Chain = Doge;
	type Callback = MockCallback<Doge>;
	type ThresholdSignatureOrigin = crate::Origin<Test, Instance1>;

	fn get_payload(&self) -> <Self::Chain as ChainCrypto>::Payload {
		self.message.clone()
	}

	fn resolve_callback(
		&self,
		signature: <Self::Chain as ChainCrypto>::ThresholdSignature,
	) -> Self::Callback {
		MockCallback(self.message.clone(), signature)
	}
}

impl pallet_cf_threshold_signature::Config<Instance1> for Test {
	type Event = Event;
	type TargetChain = Doge;
	type SigningContext = DogeThresholdSignerContext;
	type SignerNomination = MockNominator;
	type KeyProvider = MockKeyProvider;
	type OfflineReporter = MockOfflineReporter;
}

pub struct ExtBuilder {
	ext: sp_io::TestExternalities,
}

impl ExtBuilder {
	pub fn new() -> Self {
		let ext = new_test_ext();
		Self { ext }
	}

	pub fn with_nominees(mut self, nominees: impl IntoIterator<Item = u64>) -> Self {
		self.ext.execute_with(|| {
			MockNominator::set_nominees(Vec::from_iter(nominees));
		});
		self
	}

	pub fn with_validators(mut self, validators: impl IntoIterator<Item = u64>) -> Self {
		self.ext.execute_with(|| {
			MockEpochInfo::set_validators(Vec::from_iter(validators));
		});
		self
	}

	pub fn with_pending_request(mut self, message: &'static str) -> Self {
		self.ext.execute_with(|| {
			// Initiate request
			let request_id = DogeThresholdSigner::request_signature(DogeThresholdSignerContext {
				message: message.to_string(),
			});
			let pending = DogeThresholdSigner::pending_request(request_id).unwrap();
			assert_eq!(pending.attempt, 0);
			assert_eq!(
				pending.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees())
			);
		});
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		self.ext
	}
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
