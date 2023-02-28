pub mod broadcast;
mod broadcast_verification;
mod ceremony_stage;
mod failure_reason;

pub use ceremony_stage::{
	CeremonyCommon, CeremonyStage, PreProcessStageDataCheck, ProcessMessageResult, StageResult,
};

pub use broadcast_verification::BroadcastVerificationMessage;

use cf_primitives::AccountId;
pub use failure_reason::{
	BroadcastFailureReason, CeremonyFailureReason, KeygenFailureReason, SigningFailureReason,
};

use std::{
	collections::{BTreeMap, BTreeSet},
	sync::Arc,
};

use serde::{Deserialize, Serialize};

use thiserror::Error;

use crate::multisig::{
	crypto::{ECPoint, KeyShare},
	CryptoScheme,
};

use super::{signing::get_lagrange_coeff, utils::PartyIdxMapping, ThresholdParameters};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeygenResult<C: CryptoScheme> {
	#[serde(bound = "")]
	pub key_share: KeyShare<C::Point>,
	#[serde(bound = "")]
	pub party_public_keys: Vec<C::Point>,
	// NOTE: making this private ensures that the only
	// way to create the struct is through the "constructor",
	// which is important for ensuring its compatibility
	unused_private_field: (),
}

/// This computes a scalar, multiplying by which the public key will become compatible
/// according to [`crate::multisig::CryptoScheme::is_pubkey_compatible`].
fn compute_compatibility_factor<C: CryptoScheme>(
	pubkey: &C::Point,
) -> <C::Point as ECPoint>::Scalar {
	let mut factor = 1;
	let mut product = *pubkey;
	while !C::is_pubkey_compatible(&product) {
		factor += 1;
		product = product + *pubkey;
	}

	<C::Point as ECPoint>::Scalar::from(factor)
}

impl<C: CryptoScheme> KeygenResult<C> {
	/// Create keygen result, ensuring that the public key is "contract compatible" (mostly relevant
	/// for Ethereum keys/contracts, see [`crate::multisig::CryptoScheme::is_pubkey_compatible`]).
	/// Note that the keys might be modified as part of this procedure. However, the result is
	/// guaranteed to produce a valid multisig share as long as all ceremony participants use the
	/// same procedure.
	pub fn new_compatible(key_share: KeyShare<C::Point>, party_public_keys: Vec<C::Point>) -> Self {
		let factor: <C::Point as ECPoint>::Scalar = compute_compatibility_factor::<C>(&key_share.y);

		// Scale all components by `factor`, which should give us another valid multisig share
		// (w.r.t. the scaled aggregate key):
		let key_share = KeyShare { x_i: key_share.x_i * &factor, y: key_share.y * &factor };
		let party_public_keys = party_public_keys.into_iter().map(|pk| pk * &factor).collect();

		Self { key_share, party_public_keys, unused_private_field: () }
	}
}

impl<C: CryptoScheme> KeygenResult<C> {
	pub fn get_public_key(&self) -> C::Point {
		self.key_share.y
	}

	/// Gets the serialized compressed public key (33 bytes - 32 bytes + a y parity byte)
	pub fn get_public_key_bytes(&self) -> Vec<u8> {
		self.key_share.y.as_bytes().as_ref().into()
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeygenResultInfo<C: CryptoScheme> {
	#[serde(bound = "")]
	pub key: Arc<KeygenResult<C>>,
	pub validator_mapping: Arc<PartyIdxMapping>,
	pub params: ThresholdParameters,
}

/// Our own secret share and the public keys of all other participants
/// scaled by corresponding lagrange coefficients.
pub struct ResharingContext<C: CryptoScheme> {
	pub secret_share: <C::Point as ECPoint>::Scalar,
	pub party_public_keys: BTreeMap<AccountId, C::Point>,
}

impl<C: CryptoScheme> ResharingContext<C> {
	/// `participants` are a subset of the holders of the original key
	/// that are sufficient to reconstruct or, in this case, create
	/// new shares for the key.
	pub fn from_key(
		key: &KeygenResultInfo<C>,
		own_id: &AccountId,
		participants: &BTreeSet<AccountId>,
	) -> Self {
		use crate::multisig::crypto::ECScalar;
		let own_idx = key.validator_mapping.get_idx(own_id).expect("our own id must be present");

		let all_idxs: BTreeSet<_> = participants
			.iter()
			.map(|id| {
				key.validator_mapping
					.get_idx(id)
					.expect("participant must be a known key share holder")
			})
			.collect();

		// If we are not a participant, we simply set our secret to 0, otherwise
		// we use our key share scaled by the lagrange coefficient:
		let secret_share = if participants.contains(own_id) {
			get_lagrange_coeff::<C::Point>(own_idx, &all_idxs) * &key.key.key_share.x_i
		} else {
			<C::Point as ECPoint>::Scalar::zero()
		};

		let party_public_keys = key
			.validator_mapping
			.get_all_ids()
			.iter()
			.map(|id| {
				// Parties that don't "participate", are expected to set their secret to 0,
				// and thus their public key share should be a point at infinity:
				let expected_pubkey_share = if participants.contains(id) {
					let idx = key.validator_mapping.get_idx(id).expect("id must be present");
					let coeff = get_lagrange_coeff::<C::Point>(idx, &all_idxs);
					key.key.party_public_keys[idx as usize - 1] * &coeff
				} else {
					C::Point::point_at_infinity()
				};

				(id.clone(), expected_pubkey_share)
			})
			.collect();
		Self { secret_share, party_public_keys }
	}
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum KeygenStageName {
	#[error("Hash Commitments [1]")]
	HashCommitments1,
	#[error("Verify Hash Commitments [2]")]
	VerifyHashCommitmentsBroadcast2,
	#[error("Coefficient Commitments [3]")]
	CoefficientCommitments3,
	#[error("Verify Coefficient Commitments [4]")]
	VerifyCommitmentsBroadcast4,
	#[error("Secret Shares [5]")]
	SecretSharesStage5,
	#[error("Complaints [6]")]
	ComplaintsStage6,
	#[error("Verify Complaints [7]")]
	VerifyComplaintsBroadcastStage7,
	#[error("Blame Responses [8]")]
	BlameResponsesStage8,
	#[error("Verify Blame Responses [9]")]
	VerifyBlameResponsesBroadcastStage9,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum SigningStageName {
	#[error("Commitments [1]")]
	AwaitCommitments1,
	#[error("Verify Commitments [2]")]
	VerifyCommitmentsBroadcast2,
	#[error("Local Signatures [3]")]
	LocalSigStage3,
	#[error("Verify Local Signatures [4]")]
	VerifyLocalSigsBroadcastStage4,
}
