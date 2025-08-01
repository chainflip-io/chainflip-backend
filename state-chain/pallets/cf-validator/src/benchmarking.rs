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

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use pallet_cf_funding::Config as FundingConfig;
use pallet_cf_reputation::Config as ReputationConfig;
use pallet_session::Config as SessionConfig;

use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, KeyRotationStatusOuter, SafeMode, SetSafeMode};
use cf_utilities::assert_matches;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	sp_runtime::{Digest, DigestItem},
	storage_alias,
	traits::{OnNewAccount, UnfilteredDispatchable},
};
use frame_system::{pallet_prelude::OriginFor, Pallet as SystemPallet, RawOrigin};
use sp_application_crypto::RuntimeAppPublic;
use sp_std::vec;

use delegation::{DelegationAcceptance, OperatorSettings};

mod p2p_crypto {
	use sp_application_crypto::{app_crypto, ed25519, KeyTypeId};
	pub const PEER_ID_KEY: KeyTypeId = KeyTypeId(*b"peer");
	app_crypto!(ed25519, PEER_ID_KEY);
}

// For accessing missed aura slot tracking.
#[storage_alias]
type LastSeenSlot = StorageValue<AuraSlotExtraction, u64>;

pub trait RuntimeConfig: Config + FundingConfig + SessionConfig + ReputationConfig {}

impl<T: Config + FundingConfig + SessionConfig + ReputationConfig> RuntimeConfig for T {}

pub fn bidder_set<T: Chainflip, Id: From<<T as frame_system::Config>::AccountId>, I: Into<u32>>(
	size: I,
	set_id: I,
) -> impl Iterator<Item = Id> {
	let set_id = set_id.into();
	(0..size.into())
		.map(move |i| account::<<T as frame_system::Config>::AccountId>("bidder", i, set_id).into())
}

/// Initialises bidders for the auction by funding each one, registering session keys and peer ids
/// and submitting heartbeats.
pub fn init_bidders<T: RuntimeConfig>(n: u32, set_id: u32, flip_funded: u128) {
	for bidder in bidder_set::<T, <T as frame_system::Config>::AccountId, _>(n, set_id) {
		let bidder_origin: OriginFor<T> = RawOrigin::Signed(bidder.clone()).into();
		assert_ok!(pallet_cf_funding::Pallet::<T>::funded(
			T::EnsureWitnessed::try_successful_origin().unwrap(),
			bidder.clone(),
			(flip_funded * FLIPPERINOS_PER_FLIP).unique_saturated_into(),
			Default::default(),
			Default::default()
		));
		<T as frame_system::Config>::OnNewAccount::on_new_account(&bidder);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_validator(&bidder));
		assert_ok!(Pallet::<T>::start_bidding(bidder_origin.clone(),));

		let public_key: p2p_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature = public_key.sign(&bidder.encode()).unwrap();
		assert_ok!(Pallet::<T>::register_peer_id(
			bidder_origin.clone(),
			public_key.clone().into(),
			1337,
			1u128,
			signature.into(),
		));

		// Reuse the random peer id for the session keys, we don't need real ones.
		let fake_key = public_key.to_raw_vec().repeat(4);
		assert_ok!(pallet_session::Pallet::<T>::set_keys(
			bidder_origin.clone(),
			// Public key is 32 bytes, we need 128 bytes.
			T::Keys::decode(&mut &fake_key[..]).unwrap(),
			vec![],
		));

		assert_ok!(pallet_cf_reputation::Pallet::<T>::heartbeat(bidder_origin.clone(),));
	}
}

pub fn try_start_keygen<T: RuntimeConfig>(
	primary_candidates: u32,
	secondary_candidates: u32,
	epoch: u32,
) {
	// Use an offset to ensure the candidate sets don't clash.
	const LARGE_OFFSET: u32 = 100;
	init_bidders::<T>(primary_candidates, epoch, 100_000u128);
	init_bidders::<T>(secondary_candidates, epoch + LARGE_OFFSET, 90_000u128);

	Pallet::<T>::try_start_keygen(RotationState::from_auction_outcome::<T>(AuctionOutcome {
		winners: bidder_set::<T, ValidatorIdOf<T>, _>(primary_candidates, epoch).collect(),
		losers: bidder_set::<T, ValidatorIdOf<T>, _>(secondary_candidates, epoch + LARGE_OFFSET)
			.collect(),
		bond: 100u32.into(),
	}));

	assert_matches!(CurrentRotationPhase::<T>::get(), RotationPhase::KeygensInProgress(..));
}

const OPERATOR_SETTINGS: OperatorSettings =
	OperatorSettings { fee_bps: 250, delegation_acceptance: DelegationAcceptance::Allow };

#[allow(clippy::multiple_bound_locations)]
#[benchmarks(where T: RuntimeConfig)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_pallet_config() {
		let parameters = SetSizeParameters { min_size: 150, max_size: 150, max_expansion: 150 };
		let call = Call::<T>::update_pallet_config {
			update: PalletConfigUpdate::AuctionParameters { parameters },
		};
		let o = T::EnsureGovernance::try_successful_origin().unwrap();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(o));
		}

		assert_eq!(Pallet::<T>::auction_parameters(), parameters)
	}

	#[benchmark]
	fn cfe_version() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Validator,
		)
		.unwrap();

		let version = SemVer { major: 1, minor: 2, patch: 3 };

		#[extrinsic_call]
		cfe_version(RawOrigin::Signed(caller.clone()), version);

		let validator_id: ValidatorIdOf<T> = caller.into();
		assert_eq!(Pallet::<T>::node_cfe_version(validator_id), version)
	}

	#[benchmark]
	fn register_peer_id() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Validator,
		)
		.unwrap();

		let pair: p2p_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature: Ed25519Signature = pair.sign(&caller.encode()).unwrap().into();
		let public_key: Ed25519PublicKey = pair.into();

		#[extrinsic_call]
		register_peer_id(RawOrigin::Signed(caller.clone()), public_key, 0, 0, signature);

		assert!(MappedPeers::<T>::contains_key(public_key));
		assert!(AccountPeerMapping::<T>::contains_key(&caller));
	}

	#[benchmark]
	fn expire_epoch(a: Linear<3, 150>) {
		// a: the number bidders for a successful auction.

		const OLD_EPOCH: EpochIndex = 1;
		const EPOCH_TO_EXPIRE: EpochIndex = OLD_EPOCH + 1;

		let amount = T::Amount::from(1000u32);

		HistoricalBonds::<T>::insert(OLD_EPOCH, amount);
		HistoricalBonds::<T>::insert(EPOCH_TO_EXPIRE, amount);

		let authorities: Vec<_> = (0..a).map(|id| account("hello", id, id)).collect();

		HistoricalAuthorities::<T>::insert(OLD_EPOCH, authorities.clone());
		HistoricalAuthorities::<T>::insert(EPOCH_TO_EXPIRE, authorities.clone());
		for a in authorities {
			EpochHistory::<T>::activate_epoch(&a, OLD_EPOCH);
			EpochHistory::<T>::activate_epoch(&a, EPOCH_TO_EXPIRE);
		}

		// Ensure that we are expiring the expected number of authorities.
		assert_eq!(EpochHistory::<T>::epoch_authorities(EPOCH_TO_EXPIRE).len(), a as usize,);

		#[block]
		{
			Pallet::<T>::expire_epoch(EPOCH_TO_EXPIRE);
		}
	}

	#[benchmark]
	fn missed_authorship_slots(m: Linear<1, 10>) {
		// m: successive blocks missed.

		let last_slot = 1_000u64;

		SystemPallet::<T>::initialize(
			&1u32.into(),
			&SystemPallet::<T>::parent_hash(),
			&Digest { logs: vec![DigestItem::PreRuntime(*b"aura", last_slot.encode())] },
		);
		Pallet::<T>::on_initialize(1u32.into());
		assert_eq!(LastSeenSlot::get(), Some(last_slot));

		let expected_slot = last_slot + 1;
		SystemPallet::<T>::initialize(
			&1u32.into(),
			&SystemPallet::<T>::parent_hash(),
			&Digest {
				logs: vec![DigestItem::PreRuntime(*b"aura", (expected_slot + m as u64).encode())],
			},
		);

		#[block]
		{
			Pallet::<T>::punish_missed_authorship_slots();
		}

		assert_eq!(LastSeenSlot::get(), Some(expected_slot + m as u64));
	}

	/**** Rotation Benchmarks *** */

	/**** 1. RotationPhase::Idle *** */
	#[benchmark]
	fn rotation_phase_idle() {
		assert!(T::MissedAuthorshipSlots::missed_slots().is_empty());

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}

		assert_eq!(CurrentRotationPhase::<T>::get(), RotationPhase::Idle);
	}

	#[benchmark]
	fn start_authority_rotation(a: Linear<3, 400>) {
		// a = number of bidders.

		init_bidders::<T>(a, 1, 100_000u128);

		#[block]
		{
			Pallet::<T>::start_authority_rotation();
		}

		assert_matches!(CurrentRotationPhase::<T>::get(), RotationPhase::KeygensInProgress(..));
	}

	#[benchmark]
	fn start_authority_rotation_while_disabled_by_safe_mode() {
		<T as Config>::SafeMode::set_code_red();

		#[block]
		{
			Pallet::<T>::start_authority_rotation();
		}

		assert_matches!(<T as Config>::SafeMode::get(), SafeMode::CODE_RED);
		assert_matches!(CurrentRotationPhase::<T>::get(), RotationPhase::Idle);
	}

	/**** 2. RotationPhase::KeygensInProgress *** */
	#[benchmark]
	fn rotation_phase_keygen(a: Linear<3, 150>) {
		// a = authority set target size

		// Set up a vault rotation with a primary candidates and 50 auction losers (the losers just
		// have to be enough to fill up available secondary slots).
		try_start_keygen::<T>(a, 50, 1);

		// Simulate success.
		T::KeyRotator::set_status(AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete));

		// This assertion ensures we are using the correct weight parameter.
		assert_eq!(
			match CurrentRotationPhase::<T>::get() {
				RotationPhase::KeygensInProgress(rotation_state) =>
					Some(rotation_state.num_primary_candidates()),
				_ => None,
			}
			.expect("phase should be KeygensInProgress"),
			a,
			"Incorrect weight parameters."
		);

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}

		assert_matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::KeyHandoversInProgress(..)
		);
	}

	#[benchmark]
	fn rotation_phase_activating_keys(a: Linear<3, 150>) {
		// a = authority set target size

		try_start_keygen::<T>(a, 50, 1);

		let block = frame_system::Pallet::<T>::current_block_number();

		assert_matches!(CurrentRotationPhase::<T>::get(), RotationPhase::KeygensInProgress(..));

		T::KeyRotator::set_status(AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete));

		Pallet::<T>::on_initialize(block);

		assert_matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::KeyHandoversInProgress(..)
		);

		T::KeyRotator::set_status(AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete));

		Pallet::<T>::on_initialize(block);

		T::KeyRotator::set_status(AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete));

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}

		assert_matches!(CurrentRotationPhase::<T>::get(), RotationPhase::NewKeysActivated(..));
	}

	#[benchmark]
	fn register_as_validator() {
		let caller: T::AccountId = whitelisted_caller();
		frame_system::Pallet::<T>::inc_providers(&caller);
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);

		#[extrinsic_call]
		register_as_validator(RawOrigin::Signed(caller.clone()));

		assert_ok!(<T as Chainflip>::AccountRoleRegistry::ensure_validator(
			RawOrigin::Signed(caller).into()
		));
	}

	#[benchmark]
	fn deregister_as_validator() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Validator,
		)
		.unwrap();

		#[extrinsic_call]
		deregister_as_validator(RawOrigin::Signed(caller.clone()));

		assert!(<T as Chainflip>::AccountRoleRegistry::ensure_validator(
			RawOrigin::Signed(caller).into()
		)
		.is_err());
	}

	#[benchmark]
	fn stop_bidding() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		frame_system::Pallet::<T>::inc_providers(&caller);
		assert_ok!(T::AccountRoleRegistry::register_as_validator(&caller));
		ActiveBidder::<T>::set(BTreeSet::from([caller.clone()]));

		#[extrinsic_call]
		stop_bidding(RawOrigin::Signed(caller.clone()));

		assert!(!Pallet::<T>::is_bidding(&caller));
	}

	#[benchmark]
	fn start_bidding() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		frame_system::Pallet::<T>::inc_providers(&caller);
		assert_ok!(T::AccountRoleRegistry::register_as_validator(&caller));
		ActiveBidder::<T>::set(Default::default());

		#[extrinsic_call]
		start_bidding(RawOrigin::Signed(caller.clone()));

		assert!(Pallet::<T>::is_bidding(&caller));
	}
	// NOTE: Test suite not included due to missing Funding and Reputation pallet in `mock::Test`.

	#[benchmark]
	fn claim_validator() {
		let operator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Operator,
		)
		.unwrap();

		let validator = frame_benchmarking::account::<T::AccountId>("whitelisted_caller", 1, 0);
		frame_system::Pallet::<T>::inc_providers(&validator);
		<T as frame_system::Config>::OnNewAccount::on_new_account(&validator);

		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_validator(&validator));

		#[extrinsic_call]
		claim_validator(RawOrigin::Signed(operator.clone()), validator.clone());

		assert!(!ClaimedValidators::<T>::get(validator).is_empty());
	}

	#[benchmark]
	fn accept_operator() {
		let operator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Operator,
		)
		.unwrap();
		let validator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Validator,
		)
		.unwrap();

		ClaimedValidators::<T>::insert(validator.clone(), BTreeSet::from([operator.clone()]));

		#[extrinsic_call]
		accept_operator(RawOrigin::Signed(validator.clone()), operator.clone());

		assert!(ManagedValidators::<T>::get(validator).is_some());
	}

	#[benchmark]
	fn remove_validator() {
		let validator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Validator,
		)
		.unwrap();

		let operator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Operator,
		)
		.unwrap();

		ManagedValidators::<T>::insert(validator.clone(), operator.clone());

		#[extrinsic_call]
		remove_validator(RawOrigin::Signed(validator.clone()), validator.clone());

		assert!(ManagedValidators::<T>::get(validator).is_none());
	}

	#[benchmark]
	fn update_operator_settings() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Operator,
		)
		.unwrap();

		#[extrinsic_call]
		update_operator_settings(RawOrigin::Signed(caller.clone()), OPERATOR_SETTINGS);

		assert_eq!(OperatorSettingsLookup::<T>::get(caller), Some(OPERATOR_SETTINGS));
	}

	#[benchmark]
	fn block_delegator() {
		let operator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Operator,
		)
		.unwrap();

		let account_id = whitelisted_caller();

		#[extrinsic_call]
		block_delegator(RawOrigin::Signed(operator), account_id);
	}

	#[benchmark]
	fn allow_delegator() {
		let operator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Operator,
		)
		.unwrap();

		let account_id = whitelisted_caller();

		#[extrinsic_call]
		allow_delegator(RawOrigin::Signed(operator), account_id);
	}

	#[benchmark]
	fn register_as_operator() {
		let caller: T::AccountId = whitelisted_caller();
		frame_system::Pallet::<T>::inc_providers(&caller);
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);

		#[extrinsic_call]
		register_as_operator(RawOrigin::Signed(caller), OPERATOR_SETTINGS);
	}

	#[benchmark]
	fn deregister_as_operator() {
		let operator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Operator,
		)
		.unwrap();

		#[extrinsic_call]
		deregister_as_operator(RawOrigin::Signed(operator.clone()));

		assert!(<T as Chainflip>::AccountRoleRegistry::ensure_operator(
			RawOrigin::Signed(operator).into()
		)
		.is_err());
	}

	#[benchmark]
	fn delegate() {
		let operator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Operator,
		)
		.unwrap();

		assert_ok!(Pallet::<T>::update_operator_settings(
			RawOrigin::Signed(operator.clone()).into(),
			OperatorSettings { fee_bps: 250, delegation_acceptance: DelegationAcceptance::Allow }
		));

		let delegator: T::AccountId = account::<T::AccountId>("whitelisted_caller", 0, 1);
		frame_system::Pallet::<T>::inc_providers(&delegator);
		<T as frame_system::Config>::OnNewAccount::on_new_account(&delegator);

		DelegationChoice::<T>::remove(&delegator);

		#[extrinsic_call]
		delegate(RawOrigin::Signed(delegator.clone()), operator.clone());

		assert_eq!(DelegationChoice::<T>::get(delegator), Some(operator));
	}

	#[benchmark]
	fn undelegate() {
		let operator = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::Operator,
		)
		.unwrap();

		let delegator: T::AccountId = whitelisted_caller();
		frame_system::Pallet::<T>::inc_providers(&delegator);
		<T as frame_system::Config>::OnNewAccount::on_new_account(&delegator);

		DelegationChoice::<T>::insert(&delegator, operator);

		#[extrinsic_call]
		undelegate(RawOrigin::Signed(delegator.clone()));

		assert!(DelegationChoice::<T>::get(delegator).is_none());
	}
}
