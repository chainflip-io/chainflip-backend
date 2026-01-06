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

#![cfg(test)]

use super::*;
use crate as pallet_cf_validator;
use crate::PalletSafeMode;
use cf_primitives::FlipBalance;
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		cfe_interface_mock::MockCfeInterface, key_rotator::MockKeyRotatorA,
		minimum_funding::MockMinimumFundingProvider, qualify_node::QualifyAll,
		reputation_resetter::MockReputationResetter,
	},
	AccountRoleRegistry, RotationBroadcastsPending,
};
use frame_support::{construct_runtime, derive_impl, traits::HandleLifetime};
use sp_runtime::{impl_opaque_keys, testing::UintAuthorityId, traits::ConvertInto};
use std::{cell::RefCell, collections::HashMap};

use cf_traits::mocks::bonding::MockBonderFor;

pub type Amount = u128;
pub type ValidatorId = u64;

type Block = frame_system::mocking::MockBlock<Test>;

pub const MIN_AUTHORITY_SIZE: u32 = 1;
pub const MAX_AUTHORITY_SIZE: u32 = WINNING_BIDS.len() as u32;
pub const MAX_AUTHORITY_SET_EXPANSION: u32 = WINNING_BIDS.len() as u32;

pub type MockFlip = MockFundingInfo<Test>;

construct_runtime!(
	pub struct Test {
		System: frame_system,
		ValidatorPallet: pallet_cf_validator,
		Session: pallet_session,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = ValidatorId;
	type Block = Block;
}

impl_mock_chainflip!(Test);

impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub dummy: UintAuthorityId,
	}
}

impl From<UintAuthorityId> for MockSessionKeys {
	fn from(dummy: UintAuthorityId) -> Self {
		Self { dummy }
	}
}

impl pallet_session::Config for Test {
	type ShouldEndSession = ValidatorPallet;
	type SessionManager = ValidatorPallet;
	type SessionHandler = pallet_session::TestSessionHandler;
	type ValidatorId = ValidatorId;
	type ValidatorIdOf = ConvertInto;
	type Keys = MockSessionKeys;
	type RuntimeEvent = RuntimeEvent;
	type NextSessionRotation = ();
	type WeightInfo = ();
}
pub const WINNING_BIDS: [Bid<ValidatorId, FlipBalance>; 4] = [
	Bid { bidder_id: 0, amount: 120 },
	Bid { bidder_id: 1, amount: 120 },
	Bid { bidder_id: 2, amount: 110 },
	Bid { bidder_id: 3, amount: 105 },
];
pub const LOSING_BIDS: [Bid<ValidatorId, FlipBalance>; 3] = [
	Bid { bidder_id: 5, amount: 99 },
	Bid { bidder_id: 6, amount: 90 },
	Bid { bidder_id: 7, amount: 74 },
];
pub const UNQUALIFIED_BID: Bid<ValidatorId, FlipBalance> = Bid { bidder_id: 8, amount: 200 };

pub const EXPECTED_BOND: Amount = WINNING_BIDS[WINNING_BIDS.len() - 1].amount;

pub struct TestEpochTransitionHandler;

impl EpochTransitionHandler for TestEpochTransitionHandler {}

thread_local! {
	pub static MISSED_SLOTS: RefCell<(u64, u64)> = RefCell::new(Default::default());
}

pub struct MockMissedAuthorshipSlots;

impl MockMissedAuthorshipSlots {
	pub fn set(expected: u64, authored: u64) {
		MISSED_SLOTS.with(|cell| *cell.borrow_mut() = (expected, authored))
	}

	pub fn get() -> (u64, u64) {
		MISSED_SLOTS.with(|cell| *cell.borrow())
	}
}

impl MissedAuthorshipSlots for MockMissedAuthorshipSlots {
	fn missed_slots() -> sp_std::ops::Range<u64> {
		let (expected, authored) = Self::get();
		expected..authored
	}
}

thread_local! {
	pub static AUTHORITY_BONDS: RefCell<HashMap<ValidatorId, Amount>> = RefCell::new(HashMap::default());
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<ValidatorId, PalletOffence>;

pub struct MockRotationBroadcastsPending;
impl RotationBroadcastsPending for MockRotationBroadcastsPending {
	fn rotation_broadcasts_pending() -> bool {
		false
	}
}

impl_mock_runtime_safe_mode!(validator: PalletSafeMode);
impl Config for Test {
	type Offence = PalletOffence;
	type EpochTransitionHandler = TestEpochTransitionHandler;
	type KeyRotator = MockKeyRotatorA;
	type RotationBroadcastsPending = MockRotationBroadcastsPending;
	type MissedAuthorshipSlots = MockMissedAuthorshipSlots;
	type OffenceReporter = MockOffenceReporter;
	type Bonder = MockBonderFor<Self>;
	type ReputationResetter = MockReputationResetter<Self>;
	type KeygenQualification = QualifyAll<ValidatorId>;
	type SafeMode = MockRuntimeSafeMode;
	type ValidatorWeightInfo = ();
	type MinimumFunding = MockMinimumFundingProvider;
	type CfePeerRegistration = MockCfeInterface;
}

/// Session pallet requires a set of validators at genesis.
pub const GENESIS_AUTHORITIES: [u64; 3] = [u64::MAX, u64::MAX - 1, u64::MAX - 2];
pub const REDEMPTION_PERCENTAGE_AT_GENESIS: Percent = Percent::from_percent(50);
pub const GENESIS_BOND: Amount = 100;
pub const EPOCH_DURATION: u64 = 10;

fn all_validators() -> Vec<ValidatorId> {
	[
		&GENESIS_AUTHORITIES[..],
		&[&WINNING_BIDS[..], &LOSING_BIDS[..]]
			.concat()
			.into_iter()
			.map(|bid| bid.bidder_id)
			.collect::<Vec<_>>()[..],
	]
	.concat()
	.to_vec()
}

pub const ALICE: u64 = 100;
pub const BOB: u64 = 101;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: SystemConfig::default(),
		session: SessionConfig {
			keys: all_validators()
				.into_iter()
				.map(|i| (i, i, UintAuthorityId(i).into()))
				.collect(),
		},
		validator_pallet: ValidatorPalletConfig {
			genesis_authorities: BTreeSet::from(GENESIS_AUTHORITIES),
			epoch_duration: EPOCH_DURATION,
			bond: GENESIS_BOND,
			redemption_period_as_percentage: REDEMPTION_PERCENTAGE_AT_GENESIS,
			authority_set_min_size: MIN_AUTHORITY_SIZE,
			auction_parameters: SetSizeParameters {
				min_size: MIN_AUTHORITY_SIZE,
				max_size: MAX_AUTHORITY_SIZE,
				max_expansion: MAX_AUTHORITY_SET_EXPANSION,
			},
			max_authority_set_contraction_percentage: DEFAULT_MAX_AUTHORITY_SET_CONTRACTION,
		},
	},
	||{
		for account_id in all_validators()
		{
			frame_system::Provider::<Test>::created(&account_id).unwrap();
			<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&account_id).unwrap();
		}
		frame_system::Provider::<Test>::created(&ALICE).unwrap();
		frame_system::Provider::<Test>::created(&BOB).unwrap();
		// Account creation is necessary but it emits events. We clear them so that
		// they don't interfere with event-based tests.
		frame_system::Pallet::<Test>::reset_events();
	},
}

#[macro_export]
macro_rules! assert_invariants {
	() => {
		assert_eq!(
			<ValidatorPallet as EpochInfo>::current_authorities(),
			Session::validators(),
			"Authorities out of sync at block {:?}. RotationPhase: {:?}",
			System::block_number(),
			ValidatorPallet::current_rotation_phase(),
		);

		// Each authority should EITHER:
		// own a delegation snapshot with no delegators.
		// OR
		// be a managed validator with a delegation snapshot that contains the authority as a
		// validator.
		let all_managed_validators = DelegationSnapshots::<Test>::iter_prefix(
			ValidatorPallet::epoch_index(),
		).flat_map(|(_, snapshot)| snapshot.validators.keys().cloned().collect::<Vec<_>>())
		.collect::<Vec<_>>();
		// Ensure no duplicates in managed validators
		let all_managed_validators_set: BTreeSet<ValidatorId> =
			BTreeSet::from_iter(all_managed_validators.iter().cloned());
		assert_eq!(
			all_managed_validators_set.len(),
			all_managed_validators.len(),
			"Duplicate validators found in managed validators"
		);
		// Each validator in the snapshot should be resolvable to a snapshot that contains them as a validator.
		let epoch_index = ValidatorPallet::epoch_index();
		for managed_validator in all_managed_validators_set {
			// Find the operator for this validator
			let operator = ValidatorToOperator::<Test>::get(epoch_index, &managed_validator)
				.expect(&format!(
					"Managed validator {:?} has no operator mapping at block {:?}",
					managed_validator,
					System::block_number()
				));
			let snapshot = DelegationSnapshots::<Test>::get(epoch_index, &operator)
				.expect(&format!(
					"Operator {:?} for validator {:?} has no delegation snapshot at block {:?}",
					operator,
					managed_validator,
					System::block_number()
				));
			assert!(
				snapshot.validators.contains_key(&managed_validator),
				"Managed validator {:?} not found in their own delegation snapshot {:?} at block {:?}",
				managed_validator,
				snapshot,
				System::block_number()
			);
		}
		for authority in ValidatorPallet::current_authorities() {
			// Check if authority is managed by an operator
			if let Some(operator) = ValidatorToOperator::<Test>::get(epoch_index, &authority) {
				let snapshot = DelegationSnapshots::<Test>::get(epoch_index, &operator)
					.expect(&format!(
						"Operator {:?} for authority {:?} has no delegation snapshot at block {:?}",
						operator,
						authority,
						System::block_number()
					));
				assert!(
					snapshot.validators.contains_key(&authority),
					"Invalid snapshot {:?} for authority {:?} at block {:?}",
					snapshot,
					authority,
					System::block_number()
				);
			}
		}
	};
}

/// Traits for helper functions used in tests
pub trait TestHelper {
	fn then_execute_with_checks<R>(self, execute: impl FnOnce() -> R) -> TestRunner<R>;
	fn then_advance_n_blocks_and_execute_with_checks<R>(
		self,
		block: BlockNumberFor<Test>,
		execute: impl FnOnce() -> R,
	) -> TestRunner<R>;
}

impl<Ctx: Clone> TestHelper for TestRunner<Ctx> {
	/// Run checks before and after the execution to ensure the integrity of states.
	fn then_execute_with_checks<R>(self, execute: impl FnOnce() -> R) -> TestRunner<R> {
		self.then_execute_with(|_| {
			QualifyAll::<u64>::except([UNQUALIFIED_BID.bidder_id]);
			log::debug!("Pre-test invariant check.");
			assert_invariants!();
			log::debug!("Pre-test invariant check passed.");
			let r = execute();
			log::debug!("Post-test invariant check.");
			assert_invariants!();
			r
		})
	}

	/// Run forward certain number of blocks, then execute with checks before and after.
	/// All hooks are run for each block forwarded.
	fn then_advance_n_blocks_and_execute_with_checks<R>(
		self,
		blocks: BlockNumberFor<Test>,
		execute: impl FnOnce() -> R,
	) -> TestRunner<R> {
		self.then_execute_with(|_| System::current_block_number() + blocks)
			.then_process_blocks_until(|execution_block| {
				assert_invariants!();
				System::current_block_number() == execution_block - 1
			})
			.then_execute_at_next_block(|_| {
				QualifyAll::<u64>::except([UNQUALIFIED_BID.bidder_id]);
				log::debug!("Pre-test invariant check.");
				assert_invariants!();
				let r = execute();
				log::debug!("Post-test invariant check.");
				assert_invariants!();
				r
			})
	}
}
