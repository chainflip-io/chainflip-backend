// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::{cell::RefCell, collections::BTreeSet, marker::PhantomData};

use crate::{
	self as pallet_cf_threshold_signature, Call, CeremonyIdCounter, CeremonyRetryQueues,
	EnsureThresholdSigned, Origin, Pallet, PalletOffence, PendingCeremonies, RequestId,
};
use cf_chains::{
	mocks::{MockAggKey, MockEthereumChainCrypto, MockThresholdSignature},
	ChainCrypto,
};
use cf_primitives::{AuthorityCount, CeremonyId, FlipBalance, FLIPPERINOS_PER_FLIP, GENESIS_EPOCH};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{cfe_interface_mock::MockCfeInterface, signer_nomination::MockNominator},
	AccountRoleRegistry, AsyncResult, KeyProvider, Slashing, StartKeyActivationResult,
	ThresholdSigner, VaultActivator,
};
use cf_utilities::assert_matches;
use codec::{Decode, Encode};
use frame_support::dispatch::DispatchResultWithPostInfo;
pub use frame_support::{
	derive_impl,
	instances::Instance1,
	parameter_types,
	traits::{EnsureOrigin, UnfilteredDispatchable},
};
use frame_system::{self, pallet_prelude::BlockNumberFor};
use scale_info::TypeInfo;

type Block = frame_system::mocking::MockBlock<Test>;

pub type ValidatorId = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		EvmThresholdSigner: pallet_cf_threshold_signature::<Instance1>,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

thread_local! {
	pub static CALL_DISPATCHED: std::cell::RefCell<Option<RequestId>> = Default::default();
	pub static TIMES_CALLED: std::cell::RefCell<u8> = Default::default();
	pub static SLASHES: RefCell<Vec<u64>> = RefCell::new(Default::default());
	pub static VAULT_ACTIVATION_STATUS: RefCell<AsyncResult<()>> = RefCell::new(AsyncResult::Pending);
}
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum MockCallback<C: ChainCrypto> {
	Regular(RequestId, PhantomData<C>),
	Keygen(Call<Test, Instance1>),
}

impl<C: ChainCrypto> Default for MockCallback<C> {
	fn default() -> Self {
		Self::Regular(Default::default(), Default::default())
	}
}

impl MockCallback<MockEthereumChainCrypto> {
	pub fn new(id: RequestId) -> Self {
		Self::Regular(id, Default::default())
	}

	pub fn call(self) {
		match self {
			Self::Regular(request_id, _) => {
				assert_matches!(
					<EvmThresholdSigner as ThresholdSigner<_>>::signature_result(request_id).1,
					AsyncResult::Ready(..)
				);
				CALL_DISPATCHED.with(|cell| *(cell.borrow_mut()) = Some(request_id));
			},
			Self::Keygen(call) => {
				_ = call.dispatch_bypass_filter(Origin(Default::default()).into());
				CALL_DISPATCHED.with(|cell| *(cell.borrow_mut()) = Some(999));
			},
		}
		TIMES_CALLED.with(|times| *times.borrow_mut() += 1)
	}

	pub fn has_executed(id: RequestId) -> bool {
		CALL_DISPATCHED.with(|cell| *cell.borrow()) == Some(id)
	}

	pub fn times_called() -> u8 {
		TIMES_CALLED.with(|cell| *cell.borrow())
	}
}

impl UnfilteredDispatchable for MockCallback<MockEthereumChainCrypto> {
	type RuntimeOrigin = RuntimeOrigin;

	fn dispatch_bypass_filter(self, origin: Self::RuntimeOrigin) -> DispatchResultWithPostInfo {
		EnsureThresholdSigned::<Test, Instance1>::ensure_origin(origin)?;
		self.call();
		Ok(().into())
	}
}

impl From<Call<Test, Instance1>> for MockCallback<MockEthereumChainCrypto> {
	fn from(value: Call<Test, Instance1>) -> Self {
		Self::Keygen(value)
	}
}

pub fn current_agg_key() -> <MockEthereumChainCrypto as ChainCrypto>::AggKey {
	<Pallet<Test, Instance1> as KeyProvider<
		<Test as pallet_cf_threshold_signature::Config<Instance1>>::TargetChainCrypto,
	>>::active_epoch_key()
	.unwrap()
	.key
}

pub fn sign(
	payload: <MockEthereumChainCrypto as ChainCrypto>::Payload,
	key: <MockEthereumChainCrypto as ChainCrypto>::AggKey,
) -> MockThresholdSignature<
	<MockEthereumChainCrypto as ChainCrypto>::AggKey,
	<MockEthereumChainCrypto as ChainCrypto>::Payload,
> {
	MockThresholdSignature::<_, _> { signing_key: key, signed_payload: payload }
}

pub const INVALID_SIGNATURE: <MockEthereumChainCrypto as ChainCrypto>::ThresholdSignature =
	MockThresholdSignature::<_, _> { signing_key: MockAggKey(*b"BAD!"), signed_payload: *b"BAD!" };

parameter_types! {
	pub const CeremonyRetryDelay: BlockNumberFor<Test> = 4;
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<u64, PalletOffence>;

impl_mock_runtime_safe_mode! { threshold_signature: pallet_cf_threshold_signature::PalletSafeMode<Instance1> }

impl pallet_cf_threshold_signature::Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type Offence = PalletOffence;
	type RuntimeOrigin = RuntimeOrigin;
	type ThresholdCallable = MockCallback<MockEthereumChainCrypto>;
	type TargetChainCrypto = MockEthereumChainCrypto;
	type ThresholdSignerNomination = MockNominator;
	type VaultActivator = MockVaultActivator;
	type OffenceReporter = MockOffenceReporter;
	type CeremonyRetryDelay = CeremonyRetryDelay;
	type Slasher = MockSlasher;
	type SafeMode = MockRuntimeSafeMode;
	type CfeMultisigRequest = MockCfeInterface;
	type Weights = ();
}

pub struct MockVaultActivator;
impl VaultActivator<MockEthereumChainCrypto> for MockVaultActivator {
	type ValidatorId = <Test as Chainflip>::ValidatorId;
	fn start_key_activation(
		_new_key: MockAggKey,
		_maybe_old_key: Option<MockAggKey>,
	) -> Vec<StartKeyActivationResult> {
		VAULT_ACTIVATION_STATUS.with(|value| *(value.borrow_mut()) = AsyncResult::Pending);
		let ceremony_id = current_ceremony_id();
		vec![StartKeyActivationResult::Normal(ceremony_id as u32)]
	}

	fn status() -> AsyncResult<()> {
		VAULT_ACTIVATION_STATUS.with(|value| *value.borrow())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<()>) {
		VAULT_ACTIVATION_STATUS.with(|value| *(value.borrow_mut()) = outcome)
	}

	fn activate_key() {
		VAULT_ACTIVATION_STATUS.with(|value| *(value.borrow_mut()) = AsyncResult::Ready(()))
	}
}

impl MockVaultActivator {
	pub fn set_activation_completed() {
		VAULT_ACTIVATION_STATUS.with(|value| *(value.borrow_mut()) = AsyncResult::Ready(()))
	}
}

pub struct MockSlasher;

impl MockSlasher {
	pub fn slash_count(validator_id: ValidatorId) -> usize {
		SLASHES.with(|slashes| slashes.borrow().iter().filter(|id| **id == validator_id).count())
	}
}

impl Slashing for MockSlasher {
	type AccountId = ValidatorId;
	type BlockNumber = u64;
	type Balance = u128;

	fn slash(validator_id: &Self::AccountId, _blocks: Self::BlockNumber) {
		// Count those slashes
		SLASHES.with(|count| {
			count.borrow_mut().push(*validator_id);
		});
	}

	fn slash_balance(account_id: &Self::AccountId, _amount: FlipBalance) {
		// Count those slashes
		SLASHES.with(|count| {
			count.borrow_mut().push(*account_id);
		});
	}

	fn calculate_slash_amount(
		_account_id: &Self::AccountId,
		_blocks: Self::BlockNumber,
	) -> Self::Balance {
		unimplemented!()
	}
}

pub fn current_ceremony_id() -> CeremonyId {
	CeremonyIdCounter::<Test, _>::get()
}

pub const AGG_KEY: [u8; 4] = *b"AKEY";

/// Define helper functions used for tests.
pub trait TestHelper {
	fn with_nominees(self, nominees: impl IntoIterator<Item = u64>) -> Self;
	fn with_authorities(self, validators: impl IntoIterator<Item = u64>) -> Self;
	fn with_request(self, message: &<MockEthereumChainCrypto as ChainCrypto>::Payload) -> Self;
	fn with_request_and_callback(
		self,
		message: &<MockEthereumChainCrypto as ChainCrypto>::Payload,
		callback_gen: impl Fn(RequestId) -> MockCallback<MockEthereumChainCrypto>,
	) -> Self;
	fn execute_with_consistency_checks<R>(self, f: impl FnOnce() -> R) -> TestRunner<R>;
	fn do_consistency_check();
}
impl TestHelper for TestRunner<()> {
	fn with_nominees(self, nominees: impl IntoIterator<Item = u64>) -> Self {
		self.execute_with(|| {
			let nominees = BTreeSet::from_iter(nominees);
			MockNominator::set_nominees(if nominees.is_empty() { None } else { Some(nominees) });
		})
	}

	fn with_authorities(self, validators: impl IntoIterator<Item = u64>) -> Self {
		self.execute_with(|| {
			let validators = Vec::from_iter(validators);
			for id in &validators {
				<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id)
					.unwrap();
			}
			MockEpochInfo::set_authorities(validators);
		})
	}

	fn with_request(self, message: &<MockEthereumChainCrypto as ChainCrypto>::Payload) -> Self {
		self.execute_with(|| {
			let initial_ceremony_id = current_ceremony_id();
			// Initiate request
			let request_id =
				<EvmThresholdSigner as ThresholdSigner<_>>::request_signature(*message);
			let ceremony_id = current_ceremony_id();

			let maybe_pending_ceremony = EvmThresholdSigner::pending_ceremonies(ceremony_id);
			assert!(
				maybe_pending_ceremony.is_some() !=
					EvmThresholdSigner::pending_requests(request_id).is_some(),
					"The request should be either a pending ceremony OR a pending request at this point"
			);
			if let Some(pending_ceremony) = maybe_pending_ceremony {
				assert_eq!(
					pending_ceremony.remaining_respondents,
					BTreeSet::from_iter(MockNominator::get_nominees().unwrap_or_default())
				);
				assert_eq!(current_ceremony_id(), initial_ceremony_id + 1);
			} else {
				assert_eq!(current_ceremony_id(), initial_ceremony_id);
			}

			assert_matches!(
				EvmThresholdSigner::signer_and_signature(request_id).unwrap().signature_result,
				AsyncResult::Pending
			);
		})
	}

	fn with_request_and_callback(
		self,
		message: &<MockEthereumChainCrypto as ChainCrypto>::Payload,
		callback_gen: impl Fn(RequestId) -> MockCallback<MockEthereumChainCrypto>,
	) -> Self {
		self.execute_with(|| {
			// Initiate request
			let request_id =
				EvmThresholdSigner::request_signature_with_callback(*message, callback_gen);
			let ceremony_id = current_ceremony_id();
			let pending = EvmThresholdSigner::pending_ceremonies(ceremony_id).unwrap();
			assert_eq!(
				pending.remaining_respondents,
				BTreeSet::from_iter(MockNominator::get_nominees().unwrap_or_default())
			);
			assert_matches!(
				EvmThresholdSigner::signer_and_signature(request_id).unwrap().signature_result,
				AsyncResult::Pending
			);
			assert!(EvmThresholdSigner::request_callback(request_id).is_some());
		})
	}

	fn execute_with_consistency_checks<R>(self, f: impl FnOnce() -> R) -> TestRunner<R> {
		self.execute_with(|| {
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
	fn do_consistency_check() {
		let retries =
			BTreeSet::<_>::from_iter(CeremonyRetryQueues::<Test, _>::iter_values().flatten());
		PendingCeremonies::<Test, _>::iter().for_each(|(ceremony_id, _)| {
			assert!(retries.contains(&ceremony_id));
		});
	}
}

pub const GENESIS_AGG_PUB_KEY: MockAggKey = MockAggKey(*b"genk");
pub const MOCK_KEYGEN_RESPONSE_TIMEOUT: u64 = 25;
pub const NEW_AGG_PUB_KEY_PRE_HANDOVER: MockAggKey = MockAggKey(*b"next");
pub const NEW_AGG_PUB_KEY_POST_HANDOVER: MockAggKey = MockAggKey(*b"hand");

pub const ALICE: <Test as frame_system::Config>::AccountId = 123u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <Test as frame_system::Config>::AccountId = 789u64;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system:Default::default(),
		evm_threshold_signer: EvmThresholdSignerConfig {
			key: Some(GENESIS_AGG_PUB_KEY),
			threshold_signature_response_timeout: 1,
			keygen_response_timeout: MOCK_KEYGEN_RESPONSE_TIMEOUT,
			amount_to_slash: FLIPPERINOS_PER_FLIP,
			_instance: PhantomData,
	} },
	|| {
		let authorities = Vec::from([ALICE, BOB, CHARLIE]);
		for id in &authorities {
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id)
				.unwrap();
		}
		MockEpochInfo::set_epoch(GENESIS_EPOCH);
		MockEpochInfo::set_epoch_authority_count(
			GENESIS_EPOCH,
			authorities.len() as AuthorityCount,
		);
		MockEpochInfo::set_authorities(authorities);
	}
}
