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

use super::*;
use cf_primitives::AccountRole;
use codec::Encode;
use pallet_cf_validator::GrandpaVoteDelegation;
use sp_core::Pair;

/// Returns the GRANDPA key and a delegation proof for `validator` signed by `delegate_pair`.
///
/// The validator's GRANDPA key is derived from its AccountId string — matching the convention
/// used in both the genesis session config and `network::setup_account`.
fn make_delegation_proof(
	validator: &AccountId,
	delegate_pair: &sp_core::ed25519::Pair,
) -> (GrandpaId, sp_consensus_grandpa::AuthoritySignature) {
	let grandpa_key = chainflip_node::test_account_from_seed::<GrandpaId>(&validator.to_string());
	let session_index = pallet_session::Pallet::<Runtime>::current_index();
	let payload = (validator.clone(), session_index).encode();
	(grandpa_key, sp_consensus_grandpa::AuthoritySignature::from(delegate_pair.sign(&payload)))
}

/// Exercises the full GRANDPA vote delegation lifecycle against the real runtime.
///
/// Uses a genesis validator whose session keys are registered at genesis, so
/// `pallet_session::key_owner` returns the correct owner without needing an epoch rotation.
#[test]
fn grandpa_delegation_lifecycle() {
	const EPOCH_BLOCKS: u32 = 1000;

	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.build()
		.execute_with(|| {
			let validator: AccountId = AccountId::from(ALICE);

			// ALICE's session keys were set at genesis using her AccountId string as the seed.
			let grandpa_key: GrandpaId =
				chainflip_node::test_account_from_seed::<GrandpaId>(&validator.to_string());

			// Confirm that the session pallet records ALICE as the owner of this key.
			assert_eq!(
				pallet_session::Pallet::<Runtime>::key_owner(
					sp_consensus_grandpa::KEY_TYPE,
					grandpa_key.as_ref(),
				),
				Some(validator.clone()),
			);

			// Fresh ed25519 delegate keypair — this key will cast votes on ALICE's behalf.
			let delegate_pair = sp_core::ed25519::Pair::from_seed(&[99u8; 32]);
			let delegate_key: GrandpaId = GrandpaId::from(delegate_pair.public());
			let (_, proof) = make_delegation_proof(&validator, &delegate_pair);

			// 1. Delegation succeeds with a valid proof.
			assert_ok!(Validator::delegate_grandpa_vote(
				RuntimeOrigin::signed(validator.clone()),
				grandpa_key.clone(),
				delegate_key.clone(),
				proof,
			));

			// 2. Delegation is stored in pallet_grandpa.
			assert_eq!(
				pallet_grandpa::Pallet::<Runtime>::get_delegate(&grandpa_key),
				Some(delegate_key.clone()),
			);

			// 3. set_keys is blocked while a delegation is active.
			let new_seed = "new_validator_key";
			assert_noop!(
				Validator::set_keys(
					RuntimeOrigin::signed(validator.clone()),
					state_chain_runtime::opaque::SessionKeys {
						aura: chainflip_node::test_account_from_seed::<AuraId>(new_seed),
						grandpa: chainflip_node::test_account_from_seed::<GrandpaId>(new_seed),
					},
					vec![],
				),
				pallet_cf_validator::Error::<Runtime>::GrandpaDelegationActive,
			);

			// 4. Revoke the delegation.
			assert_ok!(Validator::revoke_grandpa_delegation(
				RuntimeOrigin::signed(validator.clone()),
				grandpa_key.clone(),
			));

			// 5. Delegation is gone.
			assert_eq!(pallet_grandpa::Pallet::<Runtime>::get_delegate(&grandpa_key), None,);

			// 6. set_keys works again after revocation.
			assert_ok!(Validator::set_keys(
				RuntimeOrigin::signed(validator.clone()),
				state_chain_runtime::opaque::SessionKeys {
					aura: chainflip_node::test_account_from_seed::<AuraId>(new_seed),
					grandpa: chainflip_node::test_account_from_seed::<GrandpaId>(new_seed),
				},
				vec![],
			));
		});
}

/// Verifies that deregistering a validator automatically revokes any active GRANDPA delegation.
///
/// Uses a fresh validator that registered keys but never won an auction, so it carries no bond
/// and no historical key-holder epochs — both required for the deregistration check to pass.
#[test]
fn deregistration_auto_revokes_grandpa_delegation() {
	const EPOCH_BLOCKS: u32 = 1000;

	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.build()
		.execute_with(|| {
			// Create a fresh validator account: funded + registered + session keys set,
			// but never bidding and never a key-holder.
			let validator: AccountId = AccountId::from([0xde; 32]);
			network::new_account(&validator, AccountRole::Validator);
			network::setup_account(&validator);

			let delegate_pair = sp_core::ed25519::Pair::from_seed(&[77u8; 32]);
			let delegate_key: GrandpaId = GrandpaId::from(delegate_pair.public());
			let (grandpa_key, proof) = make_delegation_proof(&validator, &delegate_pair);

			// Confirm key ownership is visible in the current session.
			assert_eq!(
				pallet_session::Pallet::<Runtime>::key_owner(
					sp_consensus_grandpa::KEY_TYPE,
					grandpa_key.as_ref(),
				),
				Some(validator.clone()),
			);

			// Delegate the vote.
			assert_ok!(Validator::delegate_grandpa_vote(
				RuntimeOrigin::signed(validator.clone()),
				grandpa_key.clone(),
				delegate_key.clone(),
				proof,
			));
			assert_eq!(
				pallet_grandpa::Pallet::<Runtime>::get_delegate(&grandpa_key),
				Some(delegate_key),
			);

			// Deregister — this must auto-revoke the delegation before purging keys.
			assert_ok!(Validator::deregister_as_validator(RuntimeOrigin::signed(
				validator.clone()
			),));

			// The delegation must no longer exist.
			assert_eq!(pallet_grandpa::Pallet::<Runtime>::get_delegate(&grandpa_key), None,);
		});
}
