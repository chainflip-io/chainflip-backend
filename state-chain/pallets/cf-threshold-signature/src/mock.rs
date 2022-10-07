use std::{collections::BTreeSet, marker::PhantomData};

use crate::{
	self as pallet_cf_threshold_signature, CeremonyId, EnsureThresholdSigned, LiveCeremonies,
	OpenRequests, PalletOffence, RequestContext, RequestId,
};
use cf_chains::{
	mocks::{MockEthereum, MockThresholdSignature},
	ChainCrypto,
};
use cf_traits::{
	mocks::{
		ceremony_id_provider::MockCeremonyIdProvider, signer_nomination::MockNominator,
		system_state_info::MockSystemStateInfo,
	},
	AsyncResult, Chainflip, ThresholdSigner,
};
use codec::{Decode, Encode};
use frame_support::{
	instances::Instance1,
	parameter_types,
	traits::{EnsureOrigin, UnfilteredDispatchable},
};
use frame_system;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
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
		MockEthereumThresholdSigner: pallet_cf_threshold_signature::<Instance1>,
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
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

use cf_traits::mocks::{ensure_origin_mock::NeverFailingOriginCheck, epoch_info::MockEpochInfo};

impl Chainflip for Test {
	type KeyId = Vec<u8>;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = NeverFailingOriginCheck<Self>;
	type EnsureWitnessedAtCurrentEpoch = NeverFailingOriginCheck<Self>;
	type EpochInfo = MockEpochInfo;
	type SystemState = MockSystemStateInfo;
}

// Mock Callback

thread_local! {
	pub static CALL_DISPATCHED: std::cell::RefCell<Option<RequestId>> = Default::default();
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockCallback<C: ChainCrypto>(RequestId, PhantomData<C>);

impl MockCallback<MockEthereum> {
	pub fn new(id: RequestId) -> Self {
		Self(id, Default::default())
	}

	pub fn call(self) {
		assert!(matches!(
			<MockEthereumThresholdSigner as ThresholdSigner<_>>::signature_result(self.0),
			AsyncResult::Ready(..)
		));
		CALL_DISPATCHED.with(|cell| *(cell.borrow_mut()) = Some(self.0));
	}

	pub fn has_executed(id: RequestId) -> bool {
		CALL_DISPATCHED.with(|cell| *cell.borrow()) == Some(id)
	}
}

impl UnfilteredDispatchable for MockCallback<MockEthereum> {
	type Origin = Origin;

	fn dispatch_bypass_filter(
		self,
		origin: Self::Origin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		EnsureThresholdSigned::<Test, Instance1>::ensure_origin(origin)?;
		self.call();
		Ok(().into())
	}
}

// Mock KeyProvider
pub const MOCK_AGG_KEY: [u8; 4] = *b"AKEY";

pub struct MockKeyProvider;

impl cf_traits::KeyProvider<MockEthereum> for MockKeyProvider {
	type KeyId = Vec<u8>;

	fn current_key_id() -> Self::KeyId {
		MOCK_AGG_KEY.into()
	}

	fn current_key() -> <MockEthereum as ChainCrypto>::AggKey {
		MOCK_AGG_KEY
	}
}

pub fn sign(
	payload: <MockEthereum as ChainCrypto>::Payload,
) -> MockThresholdSignature<
	<MockEthereum as ChainCrypto>::AggKey,
	<MockEthereum as ChainCrypto>::Payload,
> {
	MockThresholdSignature::<_, _> { signing_key: MOCK_AGG_KEY, signed_payload: payload }
}

pub const INVALID_SIGNATURE: <MockEthereum as ChainCrypto>::ThresholdSignature =
	MockThresholdSignature::<_, _> { signing_key: *b"BAD!", signed_payload: *b"BAD!" };

parameter_types! {
	pub const CeremonyRetryDelay: <Test as frame_system::Config>::BlockNumber = 1;
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<u64, PalletOffence>;

impl pallet_cf_threshold_signature::Config<Instance1> for Test {
	type Event = Event;
	type Offence = PalletOffence;
	type RuntimeOrigin = Origin;
	type AccountRoleRegistry = ();
	type ThresholdCallable = MockCallback<MockEthereum>;
	type TargetChain = MockEthereum;
	type SignerNomination = MockNominator;
	type KeyProvider = MockKeyProvider;
	type EnsureGovernance = NeverFailingOriginCheck<Self>;
	type OffenceReporter = MockOffenceReporter;
	type CeremonyIdProvider = MockCeremonyIdProvider<CeremonyId>;
	type CeremonyRetryDelay = CeremonyRetryDelay;
	type Weights = ();
}

#[derive(Default)]
pub struct ExtBuilder {
	ext: sp_io::TestExternalities,
}

impl ExtBuilder {
	#[allow(clippy::new_without_default)]
	pub fn new() -> Self {
		let ext = new_test_ext();
		Self { ext }
	}

	pub fn with_nominees(mut self, nominees: impl IntoIterator<Item = u64>) -> Self {
		self.ext.execute_with(|| {
			let nominees = BTreeSet::from_iter(nominees);
			MockNominator::set_nominees(if nominees.is_empty() { None } else { Some(nominees) });
		});
		self
	}

	pub fn with_authorities(mut self, validators: impl IntoIterator<Item = u64>) -> Self {
		self.ext.execute_with(|| {
			MockEpochInfo::set_authorities(Vec::from_iter(validators));
		});
		self
	}

	pub fn with_request(mut self, message: &<MockEthereum as ChainCrypto>::Payload) -> Self {
		self.ext.execute_with(|| {
			// Initiate request
			let request_id =
				<MockEthereumThresholdSigner as ThresholdSigner<_>>::request_signature(*message);
			let (ceremony_id, attempt) =
				MockEthereumThresholdSigner::live_ceremonies(request_id).unwrap();
			let pending = MockEthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			assert_eq!(
				MockEthereumThresholdSigner::open_requests(ceremony_id).unwrap().payload,
				*message
			);
			assert_eq!(attempt, 0);
			assert_eq!(
				pending.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap_or_default())
			);
			assert!(matches!(
				MockEthereumThresholdSigner::signatures(request_id),
				AsyncResult::Pending
			));
		});
		self
	}

	pub fn with_request_and_callback(
		mut self,
		message: &<MockEthereum as ChainCrypto>::Payload,
		callback_gen: impl Fn(RequestId) -> MockCallback<MockEthereum>,
	) -> Self {
		self.ext.execute_with(|| {
			// Initiate request
			let request_id = MockEthereumThresholdSigner::request_signature_with_callback(
				*message,
				callback_gen,
			);
			let (ceremony_id, attempt) =
				MockEthereumThresholdSigner::live_ceremonies(request_id).unwrap();
			let pending = MockEthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			assert_eq!(
				MockEthereumThresholdSigner::open_requests(ceremony_id).unwrap().payload,
				*message
			);
			assert_eq!(attempt, 0);
			assert_eq!(
				pending.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap_or_default())
			);
			assert!(matches!(
				MockEthereumThresholdSigner::signatures(request_id),
				AsyncResult::Pending
			));
			assert!(MockEthereumThresholdSigner::request_callback(request_id).is_some());
		});
		self
	}

	pub fn build(self) -> TestExternalitiesWithCheck {
		TestExternalitiesWithCheck { ext: self.ext }
	}
}

/// Wraps the TestExternalities so that we can run consistency checks before and after each test.
pub struct TestExternalitiesWithCheck {
	ext: sp_io::TestExternalities,
}

impl TestExternalitiesWithCheck {
	pub fn execute_with<R>(&mut self, f: impl FnOnce() -> R) -> R {
		self.ext.execute_with(|| {
			Self::do_consistency_check();
			let r = f();
			Self::do_consistency_check();
			r
		})
	}

	/// Checks conditions that should always hold.
	pub fn do_consistency_check() {
		OpenRequests::<Test, _>::iter().for_each(
			|(ceremony_id, RequestContext { request_id, attempt_count, .. })| {
				assert_eq!(
					LiveCeremonies::<Test, _>::get(request_id).unwrap(),
					(ceremony_id, attempt_count)
				);
			},
		);
		LiveCeremonies::<Test, _>::iter().for_each(
			|(live_request_id, (live_ceremony_id, live_attempt_count))| {
				assert!(matches!(
					OpenRequests::<Test, _>::get(live_ceremony_id),
					Some(RequestContext { request_id, attempt_count, .. })
						if (request_id, attempt_count) == (live_request_id, live_attempt_count),
				));
			},
		);
	}
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		GenesisConfig::default().build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
