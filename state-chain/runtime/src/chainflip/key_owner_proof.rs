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
		// Any account can register session keys, so the owner must also be a validator in the
		// current session, ie. part of the GRANDPA voter set.
		let id = Session::key_owner(key.0, key.1.as_slice())
			.filter(|owner| Session::validators().contains(owner));

		match &id {
			Some(owner) => log::debug!(
				target: "grandpa",
				"Key ownership proof check for key {:?} at {:?} succeeded: owner is {:?}",
				key,
				proof,
				owner
			),
			None => log::debug!(
				target: "grandpa",
				"Key ownership proof check for key {:?} at {:?} failed: no owner found in the current session.",
				key,
				proof
			),
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
