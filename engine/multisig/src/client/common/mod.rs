pub mod broadcast;
mod broadcast_verification;
mod ceremony_stage;
mod failure_reason;
#[cfg(test)]
mod tests;

pub use ceremony_stage::{
	CeremonyCommon, CeremonyStage, PreProcessStageDataCheck, ProcessMessageResult, StageResult,
};

pub use broadcast_verification::{BroadcastVerificationMessage, DelayDeserialization};

use cf_primitives::{AccountId, AuthorityCount};
pub use failure_reason::{
	BroadcastFailureReason, CeremonyFailureReason, KeygenFailureReason, SigningFailureReason,
};
use strum_macros::EnumIter;

use std::{
	collections::{BTreeMap, BTreeSet},
	sync::Arc,
};

use serde::{Deserialize, Serialize};

use thiserror::Error;

use crate::crypto::{ChainSigning, CryptoScheme, ECPoint, KeyShare};

use super::{signing::get_lagrange_coeff, utils::PartyIdxMapping, ThresholdParameters};

pub type PublicKey<C> = <<C as ChainSigning>::CryptoScheme as CryptoScheme>::PublicKey;
pub type SigningPayload<C> = <<C as ChainSigning>::CryptoScheme as CryptoScheme>::SigningPayload;
pub type Signature<C> = <<C as ChainSigning>::CryptoScheme as CryptoScheme>::Signature;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeygenResult<C: CryptoScheme> {
	#[serde(bound = "")]
	pub key_share: KeyShare<C::Point>,
	#[serde(bound = "")]
	pub party_public_keys: BTreeMap<AccountId, C::Point>,
	// NOTE: making this private ensures that the only
	// way to create the struct is through the "constructor",
	// which is important for ensuring its compatibility
	unused_private_field: (),
}

/// This computes a scalar, multiplying by which the public key will become compatible
/// according to [`multisig::CryptoScheme::is_pubkey_compatible`].
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
	/// for Ethereum keys/contracts, see [`multisig::CryptoScheme::is_pubkey_compatible`]).
	/// Note that the keys might be modified as part of this procedure. However, the result is
	/// guaranteed to produce a valid multisig share as long as all ceremony participants use the
	/// same procedure.
	pub fn new_compatible(
		key_share: KeyShare<C::Point>,
		party_public_keys: BTreeMap<AccountId, C::Point>,
	) -> Self {
		let factor: <C::Point as ECPoint>::Scalar = compute_compatibility_factor::<C>(&key_share.y);

		// Scale all components by `factor`, which should give us another valid multisig share
		// (w.r.t. the scaled aggregate key):
		let key_share = KeyShare { x_i: key_share.x_i * &factor, y: key_share.y * &factor };
		let party_public_keys =
			party_public_keys.into_iter().map(|(id, pk)| (id, pk * &factor)).collect();

		Self { key_share, party_public_keys, unused_private_field: () }
	}
}

impl<C: CryptoScheme> KeygenResult<C> {
	pub fn get_agg_public_key_point(&self) -> C::Point {
		self.key_share.y
	}

	pub fn get_agg_public_key(&self) -> C::PublicKey {
		C::pubkey_from_point(&self.get_agg_public_key_point())
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
type SecretShare<C> = <<C as CryptoScheme>::Point as ECPoint>::Scalar;
type PublicKeyShares<C> = BTreeMap<AccountId, <C as CryptoScheme>::Point>;

/// Holds state relevant to the role in the handover ceremony.
#[derive(Debug)]
pub enum ParticipantStatus<C: CryptoScheme> {
	Sharing {
		secret_share: SecretShare<C>,
		public_key_shares: PublicKeyShares<C>,
		original_key: KeygenResultInfo<C>,
	},
	/// This becomes `NonSharingReceivedKeys` after shares are broadcast
	NonSharing,
	NonSharingReceivedKeys(PublicKeyShares<C>),
}

#[derive(Debug)]
pub struct ResharingContext<C: CryptoScheme> {
	/// Participants who contribute their (existing) secret shares
	pub sharing_participants: BTreeSet<AuthorityCount>,
	/// Participants who receive new shares
	pub receiving_participants: BTreeSet<AuthorityCount>,
	/// Whether our node is sharing and the corresponding state
	pub party_status: ParticipantStatus<C>,
	/// Indexes in future signing ceremonies (i.e. for the new set of validators)
	pub future_index_mapping: PartyIdxMapping,
}

impl<C: CryptoScheme> ResharingContext<C> {
	/// `participants` are a subset of the holders of the original key
	/// that are sufficient to reconstruct or, in this case, create
	/// new shares for the key.
	pub fn from_key(
		key: &KeygenResultInfo<C>,
		own_id: &AccountId,
		sharing_participants: &BTreeSet<AccountId>,
		receiving_participants: &BTreeSet<AccountId>,
	) -> Self {
		use crate::crypto::ECScalar;
		let own_idx = key.validator_mapping.get_idx(own_id).expect("our own id must be present");

		let all_idxs: BTreeSet<_> = sharing_participants
			.iter()
			.map(|id| {
				key.validator_mapping
					.get_idx(id)
					.expect("participant must be a known key share holder")
			})
			.collect();

		// If we are not a participant, we simply set our secret to 0, otherwise
		// we use our key share scaled by the lagrange coefficient:
		let secret_share = if sharing_participants.contains(own_id) {
			get_lagrange_coeff::<C::Point>(own_idx, &all_idxs) * &key.key.key_share.x_i
		} else {
			<C::Point as ECPoint>::Scalar::zero()
		};

		let public_key_shares = key
			.validator_mapping
			.get_all_ids()
			.iter()
			.map(|id| {
				// Parties that don't "participate", are expected to set their secret to 0,
				// and thus their public key share should be a point at infinity:
				let expected_pubkey_share = if sharing_participants.contains(id) {
					let idx = key.validator_mapping.get_idx(id).expect("id must be present");
					let coeff = get_lagrange_coeff::<C::Point>(idx, &all_idxs);
					*key.key
						.party_public_keys
						.get(id)
						.expect("should have a public key for this party") *
						&coeff
				} else {
					C::Point::point_at_infinity()
				};

				(id.clone(), expected_pubkey_share)
			})
			.collect();

		let context = ResharingContext::without_key(sharing_participants, receiving_participants);

		ResharingContext {
			party_status: ParticipantStatus::Sharing {
				secret_share,
				public_key_shares,
				original_key: key.clone(),
			},
			..context
		}
	}

	pub fn without_key(
		sharing_participants: &BTreeSet<AccountId>,
		receiving_participants: &BTreeSet<AccountId>,
	) -> Self {
		// NOTE: we need to be careful when deriving indices from ids, because
		// different ceremonies will have different idx/id mappings. In this case
		// we want indexes for upcoming handover ceremony (rather than the one
		// that generated the key to be re-shared).
		let all_ids = receiving_participants.union(sharing_participants).cloned().collect();
		let party_idx_mapping = PartyIdxMapping::from_participants(all_ids);
		let future_index_mapping =
			PartyIdxMapping::from_participants(receiving_participants.clone());

		let sharing_participants: BTreeSet<_> = sharing_participants
			.iter()
			.map(|id| party_idx_mapping.get_idx(id).expect("must exist by construction"))
			.collect();

		let receiving_participants: BTreeSet<_> = receiving_participants
			.iter()
			.map(|id| party_idx_mapping.get_idx(id).expect("must exist by construction"))
			.collect();

		ResharingContext {
			sharing_participants,
			receiving_participants,
			party_status: ParticipantStatus::NonSharing,
			future_index_mapping,
		}
	}
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, EnumIter)]
pub enum KeygenStageName {
	#[error("Public Key Shares [0]")]
	PubkeyShares0,
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

/// Try to deserialize all messages. If at least one fails,
/// return the parties for which deserialization failed.
pub fn try_deserialize<T: serde::de::DeserializeOwned>(
	messages: BTreeMap<AuthorityCount, DelayDeserialization<T>>,
) -> Result<BTreeMap<AuthorityCount, T>, BTreeSet<AuthorityCount>> {
	use itertools::Itertools as _;

	let (deserialized_messages, bad_parties): (BTreeMap<_, _>, BTreeSet<_>) = messages
		.into_iter()
		.map(|(idx, serialized_message)| match serialized_message.deserialize() {
			Ok(message) => Ok((idx, message)),
			Err(e) => {
				tracing::warn!("Failed to deserialize message from party {}: {}", idx, e);
				Err(idx)
			},
		})
		.partition_result();

	if bad_parties.is_empty() {
		Ok(deserialized_messages)
	} else {
		Err(bad_parties)
	}
}
