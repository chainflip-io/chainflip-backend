use crate::{self as pallet_cf_threshold_signature};
use cf_chains::{eth, ChainCrypto};
use cf_traits::{offline_conditions::*, Chainflip, SigningContext};
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
		DogeThresholdSigner: pallet_cf_threshold_signature::<Instance1>::{Pallet, Call, Storage, Event<T>},
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

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = MockEnsureWitnessed;
}

// Mock SignerNomination

pub struct MockNominator;
pub const RANDOM_NOMINEE: u64 = 0xc001d00d as u64;

impl cf_traits::SignerNomination for MockNominator {
	type SignerId = u64;

	fn nomination_with_seed(_seed: u64) -> Self::SignerId {
		RANDOM_NOMINEE
	}

	fn threshold_nomination_with_seed(_seed: u64) -> Vec<Self::SignerId> {
		vec![RANDOM_NOMINEE]
	}
}

// Mock Callback

thread_local! {
	pub static SIGNED_MESSAGE: std::cell::RefCell<Option<String>> = Default::default()
}

pub struct MockCallback<Ctx: SigningContext<Test>>(pub String, pub Ctx::Signature);

impl<Ctx: SigningContext<Test>> MockCallback<Ctx> {
	pub fn get_stored_callback() -> Option<String> {
		SIGNED_MESSAGE.with(|cell| cell.borrow().clone())
	}
}

impl UnfilteredDispatchable for MockCallback<DogeThresholdSignerContext> {
	type Origin = Origin;

	fn dispatch_bypass_filter(
		self,
		origin: Self::Origin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		MockEnsureWitnessed::ensure_origin(origin)?;
		SIGNED_MESSAGE
			.with(|cell| *(cell.borrow_mut()) = Some(format!("So {} Such {}", self.0, self.1)));
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

thread_local! {
	pub static REPORTED: std::cell::RefCell<Vec<u64>> = Default::default()
}

pub struct MockOfflineReporter;

impl MockOfflineReporter {
	pub fn get_reported() -> Vec<u64> {
		REPORTED.with(|cell| cell.borrow().clone())
	}
}

impl OfflineReporter for MockOfflineReporter {
	type ValidatorId = u64;

	fn report(
		_condition: OfflineCondition,
		_penalty: ReputationPoints,
		validator_id: &Self::ValidatorId,
	) -> Result<frame_support::dispatch::Weight, ReportError> {
		REPORTED.with(|cell| cell.borrow_mut().push(*validator_id));
		Ok(0)
	}
}

// Mock SigningContext

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct Doge;
impl cf_chains::Chain for Doge {
	const CHAIN_ID: cf_chains::ChainId = cf_chains::ChainId::Ethereum;
}
impl ChainCrypto for Doge {
	type AggKey = eth::AggKey;
	type Payload = [u8; 4];
	type ThresholdSignature = String;

	fn verify_threshold_signature(
		_agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		*payload == DOGE_PAYLOAD && signature == "Wow!"
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct DogeThresholdSignerContext {
	pub message: String,
}

pub const DOGE_PAYLOAD: [u8; 4] = [0xcf; 4];
pub const VALID_SIGNATURE: &'static str = "Wow!";
pub const INVALID_SIGNATURE: &'static str = "Pow!";

impl SigningContext<Test> for DogeThresholdSignerContext {
	type Chain = Doge;
	type Payload = [u8; 4];
	type Signature = String;
	type Callback = MockCallback<Self>;

	fn get_payload(&self) -> Self::Payload {
		DOGE_PAYLOAD
	}

	fn resolve_callback(&self, signature: Self::Signature) -> Self::Callback {
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

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
