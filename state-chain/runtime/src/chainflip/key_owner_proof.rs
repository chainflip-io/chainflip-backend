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
use crate::{AccountId, GrandpaId, KeyTypeId, Session};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::traits::KeyOwnerProofSystem;
use scale_info::TypeInfo;
use sp_core::ByteArray;
use sp_session::{GetSessionNumber, GetValidatorCount};

/// Proof of ownership of a GRANDPA key for the current session.
#[derive(Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, Copy, Clone, PartialEq, Eq)]
pub struct CurrentSessionProofSystem;

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, Copy, Clone, PartialEq, Eq)]
pub struct CurrentSessionProof(u32);

impl KeyOwnerProofSystem<(KeyTypeId, GrandpaId)> for CurrentSessionProofSystem {
	type IdentificationTuple = AccountId;
	type Proof = CurrentSessionProof;

	/// The proof is simply the current session index.
	fn prove(key: (KeyTypeId, GrandpaId)) -> Option<Self::Proof> {
		Session::key_owner(key.0, key.1.as_slice()).map(|owner| {
			let index = Session::current_index();
			log::debug!(
				target: "grandpa",
				"Proved key ownership for {:?} at session {}: owner is {:?}",
				key,
				index,
				owner
			);
			CurrentSessionProof(index)
		})
	}

	/// The proof must be from the current session and the key must be owned by an account in the
	/// current session for the proof to be valid.
	fn check_proof(
		key: (KeyTypeId, GrandpaId),
		CurrentSessionProof(proof): CurrentSessionProof,
	) -> Option<Self::IdentificationTuple> {
		if proof != Session::current_index() {
			log::debug!(
				target: "grandpa",
				"Key ownership proof check for key {:?} at {:?} failed: session index does not match current session.",
				key,
				proof
			);
			return None;
		}
		let id = Session::key_owner(key.0, key.1.as_slice()).map(|owner| {
			log::debug!(
				target: "grandpa",
				"Key ownership proof check for key {:?} at {:?} succeeded: owner is {:?}",
				key,
				proof,
				owner
			);
			owner
		});

		if id.is_none() {
			log::debug!(
				target: "grandpa",
				"Key ownership proof check for key {:?} at {:?} failed: no owner found.",
				key,
				proof
			);
		}

		id
	}
}

// NOTE: this is used in GRANDPA for weight calculations only.
impl GetValidatorCount for CurrentSessionProof {
	fn validator_count(&self) -> sp_session::ValidatorCount {
		Session::validators().len() as u32
	}
}

impl GetSessionNumber for CurrentSessionProof {
	fn session(&self) -> u32 {
		self.0
	}
}
