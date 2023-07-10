use std::{collections::BTreeSet, marker::PhantomData};

use crate::{
	self as pallet_cf_threshold_signature, CeremonyRetryQueues, EnsureThresholdSigned,
	PalletOffence, PendingCeremonies, RequestId,
};
use cf_chains::{
	mocks::{MockAggKey, MockEthereum, MockThresholdSignature},
	ChainCrypto,
};
use cf_traits::{
	impl_mock_chainflip,
	mocks::{
		ceremony_id_provider::MockCeremonyIdProvider, key_provider::MockKeyProvider,
		signer_nomination::MockNominator,
	},
	AccountRoleRegistry, AsyncResult, KeyProvider, ThresholdSigner,
};
use codec::{Decode, Encode};
pub use frame_support::{
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
		EthereumThresholdSigner: pallet_cf_threshold_signature::<Instance1>,
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

impl_mock_chainflip!(Test);

thread_local! {
	pub static CALL_DISPATCHED: std::cell::RefCell<Option<RequestId>> = Default::default();
	pub static TIMES_CALLED: std::cell::RefCell<u8> = Default::default();
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockCallback<C: ChainCrypto>(RequestId, PhantomData<C>);

impl MockCallback<MockEthereum> {
	pub fn new(id: RequestId) -> Self {
		Self(id, Default::default())
	}

	pub fn call(self) {
		assert!(matches!(
			<EthereumThresholdSigner as ThresholdSigner<_>>::signature_result(self.0),
			AsyncResult::Ready(..)
		));
		CALL_DISPATCHED.with(|cell| *(cell.borrow_mut()) = Some(self.0));
		TIMES_CALLED.with(|times| *times.borrow_mut() += 1)
	}

	pub fn has_executed(id: RequestId) -> bool {
		CALL_DISPATCHED.with(|cell| *cell.borrow()) == Some(id)
	}

	pub fn times_called() -> u8 {
		TIMES_CALLED.with(|cell| *cell.borrow())
	}
}

impl UnfilteredDispatchable for MockCallback<MockEthereum> {
	type RuntimeOrigin = RuntimeOrigin;

	fn dispatch_bypass_filter(
		self,
		origin: Self::RuntimeOrigin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		EnsureThresholdSigned::<Test, Instance1>::ensure_origin(origin)?;
		self.call();
		Ok(().into())
	}
}

pub fn current_agg_key() -> <MockEthereum as ChainCrypto>::AggKey {
	<Test as crate::Config<_>>::KeyProvider::active_epoch_key().unwrap().key
}

pub fn sign(
	payload: <MockEthereum as ChainCrypto>::Payload,
) -> MockThresholdSignature<
	<MockEthereum as ChainCrypto>::AggKey,
	<MockEthereum as ChainCrypto>::Payload,
> {
	MockThresholdSignature::<_, _> { signing_key: current_agg_key(), signed_payload: payload }
}

pub const INVALID_SIGNATURE: <MockEthereum as ChainCrypto>::ThresholdSignature =
	MockThresholdSignature::<_, _> { signing_key: MockAggKey(*b"BAD!"), signed_payload: *b"BAD!" };

parameter_types! {
	pub const CeremonyRetryDelay: <Test as frame_system::Config>::BlockNumber = 4;
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<u64, PalletOffence>;

impl pallet_cf_threshold_signature::Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type Offence = PalletOffence;
	type RuntimeOrigin = RuntimeOrigin;
	type ThresholdCallable = MockCallback<MockEthereum>;
	type TargetChain = MockEthereum;
	type ThresholdSignerNomination = MockNominator;
	type KeyProvider = MockKeyProvider<MockEthereum>;
	type OffenceReporter = MockOffenceReporter;
	type CeremonyIdProvider = MockCeremonyIdProvider;
	type CeremonyRetryDelay = CeremonyRetryDelay;
	type Weights = ();
}

#[derive(Default)]
pub struct ExtBuilder {
	ext: sp_io::TestExternalities,
}

pub const AGG_KEY: [u8; 4] = *b"AKEY";

impl ExtBuilder {
	#[allow(clippy::new_without_default)]
	pub fn new() -> Self {
		let mut ext = new_test_ext();
		ext.execute_with(|| MockKeyProvider::<MockEthereum>::add_key(MockAggKey(AGG_KEY)));
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
			let validators = BTreeSet::from_iter(validators);
			for id in &validators {
				<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id)
					.unwrap();
			}
			MockEpochInfo::set_authorities(validators);
		});
		self
	}

	pub fn with_request(mut self, message: &<MockEthereum as ChainCrypto>::Payload) -> Self {
		self.ext.execute_with(|| {
			let initial_ceremony_id = MockCeremonyIdProvider::get();
			// Initiate request
			let request_id =
				<EthereumThresholdSigner as ThresholdSigner<_>>::request_signature(*message);
			let ceremony_id = MockCeremonyIdProvider::get();

			let maybe_pending_ceremony = EthereumThresholdSigner::pending_ceremonies(ceremony_id);
			assert!(
				maybe_pending_ceremony.is_some() !=
					EthereumThresholdSigner::pending_requests(request_id).is_some(),
					"The request should be either a pending ceremony OR a pending request at this point"
			);
			if let Some(pending_ceremony) = maybe_pending_ceremony {
				assert_eq!(
					pending_ceremony.remaining_respondents,
					BTreeSet::from_iter(MockNominator::get_nominees().unwrap_or_default())
				);
				assert_eq!(MockCeremonyIdProvider::get(), initial_ceremony_id + 1);
			} else {
				assert_eq!(MockCeremonyIdProvider::get(), initial_ceremony_id);
			}

			assert!(matches!(EthereumThresholdSigner::signature(request_id), AsyncResult::Pending));
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
			let request_id =
				EthereumThresholdSigner::request_signature_with_callback(*message, callback_gen);
			let ceremony_id = MockCeremonyIdProvider::get();
			let pending = EthereumThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			assert_eq!(
				pending.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap_or_default())
			);
			assert!(matches!(EthereumThresholdSigner::signature(request_id), AsyncResult::Pending));
			assert!(EthereumThresholdSigner::request_callback(request_id).is_some());
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
	///
	/// Every ceremony in OpenRequests should always have a corresponding entry in LiveCeremonies.
	/// Every ceremony should also have at least one retry scheduled.
	pub fn do_consistency_check() {
		let retries =
			BTreeSet::<_>::from_iter(CeremonyRetryQueues::<Test, _>::iter_values().flatten());
		PendingCeremonies::<Test, _>::iter().for_each(|(ceremony_id, _)| {
			assert!(retries.contains(&ceremony_id));
		});
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
