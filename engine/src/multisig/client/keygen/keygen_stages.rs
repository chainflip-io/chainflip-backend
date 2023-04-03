use std::{
	collections::{BTreeMap, BTreeSet},
	sync::Arc,
};

use crate::multisig::{
	client::{
		self,
		ceremony_manager::KeygenCeremony,
		common::{
			BroadcastFailureReason, KeygenFailureReason, KeygenStageName, ParticipantStatus,
			ResharingContext,
		},
		utils::find_frequent_element,
		KeygenResult, KeygenResultInfo,
	},
	crypto::ECScalar,
};

use async_trait::async_trait;
use cf_primitives::AuthorityCount;
use client::{
	common::{
		broadcast::{verify_broadcasts, BroadcastStage, BroadcastStageProcessor, DataToSend},
		CeremonyCommon, StageResult,
	},
	keygen, ThresholdParameters,
};
use itertools::Itertools;
use sp_core::H256;
use tracing::{debug, warn};
use utilities::threshold_from_share_count;

use crate::multisig::crypto::{CryptoScheme, ECPoint, KeyShare};

use keygen::{
	keygen_data::{
		BlameResponse8, CoeffComm3, Complaints6, SecretShare5, VerifyCoeffComm4, VerifyComplaints7,
	},
	keygen_detail::{
		derive_aggregate_pubkey, generate_shares_and_commitment, validate_commitments,
		verify_share, DKGCommitment, DKGUnverifiedCommitment, IncomingShares, OutgoingShares,
	},
};

use super::{
	keygen_data::{HashComm1, PubkeyShares0, VerifyBlameResponses9, VerifyHashComm2},
	keygen_detail::{
		compute_secret_key_share, derive_local_pubkeys_for_parties, generate_hash_commitment,
		ShamirShare, SharingParameters, ValidAggregateKey,
	},
	HashContext,
};

type KeygenStageResult<Crypto> = StageResult<KeygenCeremony<Crypto>>;

/// This stage is used in Key Handover ceremonies only
/// to ensure that all parties have public key shares
/// of the key that is being handed over.
pub struct PubkeySharesStage0<Crypto: CryptoScheme> {
	common: CeremonyCommon,
	keygen_context: HashContext,
	resharing_context: ResharingContext<Crypto>,
}

derive_display_as_type_name!(PubkeySharesStage0<Crypto: CryptoScheme>);

impl<Crypto: CryptoScheme> PubkeySharesStage0<Crypto> {
	pub fn new(
		common: CeremonyCommon,
		keygen_context: HashContext,
		resharing_context: ResharingContext<Crypto>,
	) -> Self {
		Self { common, keygen_context, resharing_context }
	}
}

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for PubkeySharesStage0<Crypto>
{
	type Message = PubkeyShares0<Crypto::Point>;
	const NAME: KeygenStageName = KeygenStageName::HashCommitments1;

	fn init(&mut self) -> DataToSend<Self::Message> {
		let ResharingContext { sharing_participants, party_status, .. } = &self.resharing_context;

		// If we are a sharing party, we broadcast public key shares.
		// (Otherwise, broadcast an empty struct.)
		let pubkey_shares =
			if self.resharing_context.sharing_participants.contains(&self.common.own_idx) {
				let public_key_shares = match party_status {
					ParticipantStatus::Sharing(_, pubkeys) => pubkeys,
					_ => panic!("must be a sharing party"),
				};

				sharing_participants
					.iter()
					.copied()
					.map(|idx| {
						let id = self.common.validator_mapping.get_id(idx);
						let pubkey = public_key_shares.get(id).unwrap();
						(idx, *pubkey)
					})
					.collect()
			} else {
				BTreeMap::new()
			};

		DataToSend::Broadcast(PubkeyShares0(pubkey_shares))
	}

	async fn process(
		self,
		pubkey_shares: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> StageResult<KeygenCeremony<Crypto>> {
		// NOTE: This stage resembles a broadcast verification stage, but has a subtle
		// difference in that we don't expect honest parties to disagree on the message
		// (this is because the initial messages aren't broadcast, but loaded from memory/db).

		// First of all, ignore messages sent by non-sharing parties, and
		// ignore the messages that haven't been received (i.e. `None`):
		let pubkey_shares: BTreeMap<_, _> = pubkey_shares
			.into_iter()
			.filter_map(|(idx, message)| {
				if let Some(message) = message {
					if self.resharing_context.sharing_participants.contains(&idx) {
						Some((idx, message))
					} else {
						None
					}
				} else {
					None
				}
			})
			.collect();

		let threshold =
			threshold_from_share_count(self.resharing_context.sharing_participants.len() as u32)
				as usize;

		if pubkey_shares.len() <= threshold {
			// Similar to broadcast verification stages, we are not able to report
			// any party if we don't receive enough messages.
			return StageResult::Error(
				BTreeSet::new(),
				KeygenFailureReason::BroadcastFailure(
					BroadcastFailureReason::InsufficientMessages,
					Self::NAME,
				),
			)
		}

		// At this point we should have enough messages to proceed as long as all parties are
		// honest. If we fail, it must be that some parties are sending invalid messages
		// maliciously.
		if let Some(pubkey_shares) = find_frequent_element(pubkey_shares.into_values(), threshold) {
			// Check that the pubkey shares are from the parties that we expect:
			if pubkey_shares.0.keys().copied().collect::<BTreeSet<_>>() !=
				self.resharing_context.sharing_participants
			{
				// TODO: better error?
				return StageResult::Error(
					BTreeSet::new(),
					KeygenFailureReason::BroadcastFailure(
						BroadcastFailureReason::Inconsistency,
						Self::NAME,
					),
				)
			}

			let mut resharing_context = self.resharing_context;
			// TODO: if we already have the key, ensure that it matches the received shares
			// (this is more of a sanity check rather than a requirement for the protocol.)
			if let ParticipantStatus::NonSharing = resharing_context.party_status {
				resharing_context.party_status = ParticipantStatus::NonSharingReceivedKeys(
					pubkey_shares
						.0
						.into_iter()
						.map(|(idx, share)| {
							let id = self.common.validator_mapping.get_id(idx);
							(id.clone(), share)
						})
						.collect(),
				)
			}

			let processor = HashCommitments1::new(KeygenCommon::new(
				self.common.clone(),
				self.keygen_context,
				Some(resharing_context),
			));

			let stage = BroadcastStage::new(processor, self.common);

			StageResult::NextStage(Box::new(stage))
		} else {
			// We are not really able to report parties sending inconsistent messages
			// because we only know what the message (i.e. the public key shares)
			// should be if we already have the original key.
			// TODO: report parties here if *do* have the original key and the proportion
			// of "key holders" in the ceremony is high enough for this to make a difference.
			StageResult::Error(
				BTreeSet::new(),
				KeygenFailureReason::BroadcastFailure(
					BroadcastFailureReason::Inconsistency,
					Self::NAME,
				),
			)
		}
	}
}
pub struct KeygenCommon<Crypto: CryptoScheme> {
	common: CeremonyCommon,
	/// Context to prevent replay attacks
	keygen_context: HashContext,
	resharing_context: Option<ResharingContext<Crypto>>,
	sharing_params: SharingParameters,
}

impl<Crypto: CryptoScheme> KeygenCommon<Crypto> {
	pub fn new(
		common: CeremonyCommon,
		keygen_context: HashContext,
		resharing_context: Option<ResharingContext<Crypto>>,
	) -> Self {
		// NOTE: Threshold parameters for the future key don't always match that
		// of the current key. They depend on the number of future key holders
		let share_count = if let Some(context) = &resharing_context {
			context.receiving_participants.len()
		} else {
			common.all_idxs.len()
		};

		let new_key_params = ThresholdParameters::from_share_count(
			share_count.try_into().expect("too many parties"),
		);

		let sharing_params = if let Some(context) = &resharing_context {
			SharingParameters::for_key_handover(new_key_params, context, &common.validator_mapping)
		} else {
			SharingParameters::for_keygen(new_key_params)
		};

		KeygenCommon { common, keygen_context, resharing_context, sharing_params }
	}
}

pub struct HashCommitments1<Crypto: CryptoScheme> {
	keygen_common: KeygenCommon<Crypto>,
	own_commitment: DKGUnverifiedCommitment<Crypto::Point>,
	hash_commitment: H256,
	shares: OutgoingShares<Crypto::Point>,
}

derive_display_as_type_name!(HashCommitments1<Crypto: CryptoScheme>);

impl<Crypto: CryptoScheme> HashCommitments1<Crypto> {
	pub fn new(mut keygen_common: KeygenCommon<Crypto>) -> Self {
		// Generate the secret polynomial and commit to it by hashing all public coefficients

		let zero_scalar = ECScalar::zero();
		let secret_share =
			keygen_common
				.resharing_context
				.as_ref()
				.map(|context| match &context.party_status {
					ParticipantStatus::Sharing(secret, _) => secret,
					ParticipantStatus::NonSharing => panic!("invalid stage at this point"),
					// NOTE: non-sharing parties send the dummy value of 0 as their secret share,
					ParticipantStatus::NonSharingReceivedKeys(_) => &zero_scalar,
				});

		let common = &mut keygen_common.common;

		let (shares, own_commitment) = generate_shares_and_commitment(
			&mut common.rng,
			&keygen_common.keygen_context,
			common.own_idx,
			&keygen_common.sharing_params,
			secret_share,
		);

		let hash_commitment = generate_hash_commitment(&own_commitment);

		HashCommitments1 { keygen_common, own_commitment, hash_commitment, shares }
	}
}

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for HashCommitments1<Crypto>
{
	type Message = HashComm1;
	const NAME: KeygenStageName = KeygenStageName::HashCommitments1;

	fn init(&mut self) -> DataToSend<Self::Message> {
		// We don't want to reveal the public coefficients yet, so sending the hash commitment only
		DataToSend::Broadcast(HashComm1(self.hash_commitment))
	}

	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> StageResult<KeygenCeremony<Crypto>> {
		// Prepare for broadcast verification
		let common = self.keygen_common.common.clone();
		let processor = VerifyHashCommitmentsBroadcast2 {
			keygen_common: self.keygen_common,
			own_commitment: self.own_commitment,
			hash_commitments: messages,
			shares_to_send: self.shares,
		};

		let stage = BroadcastStage::new(processor, common);

		StageResult::NextStage(Box::new(stage))
	}
}

pub struct VerifyHashCommitmentsBroadcast2<Crypto: CryptoScheme> {
	keygen_common: KeygenCommon<Crypto>,
	own_commitment: DKGUnverifiedCommitment<Crypto::Point>,
	hash_commitments: BTreeMap<AuthorityCount, Option<HashComm1>>,
	shares_to_send: OutgoingShares<Crypto::Point>,
}

#[cfg(test)]
impl<Crypto: CryptoScheme> VerifyHashCommitmentsBroadcast2<Crypto> {
	pub fn new(
		keygen_common: KeygenCommon<Crypto>,
		own_commitment: DKGUnverifiedCommitment<Crypto::Point>,
		hash_commitments: BTreeMap<AuthorityCount, Option<HashComm1>>,
		shares_to_send: OutgoingShares<Crypto::Point>,
	) -> Self {
		Self { keygen_common, own_commitment, hash_commitments, shares_to_send }
	}
}

derive_display_as_type_name!(VerifyHashCommitmentsBroadcast2<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for VerifyHashCommitmentsBroadcast2<Crypto>
{
	type Message = VerifyHashComm2;
	const NAME: KeygenStageName = KeygenStageName::VerifyHashCommitmentsBroadcast2;

	fn init(&mut self) -> DataToSend<Self::Message> {
		DataToSend::Broadcast(VerifyHashComm2 { data: self.hash_commitments.clone() })
	}

	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> StageResult<KeygenCeremony<Crypto>> {
		let hash_commitments = match verify_broadcasts(messages) {
			Ok(hash_commitments) => hash_commitments,
			Err((reported_parties, abort_reason)) => {
				warn!("Broadcast verification is not successful for {}", Self::NAME);
				return KeygenStageResult::Error(
					reported_parties,
					KeygenFailureReason::BroadcastFailure(abort_reason, Self::NAME),
				)
			},
		};

		debug!("{} is successful", Self::NAME);

		// Just saving hash commitments for now. We will use them
		// once the parties reveal their public coefficients (next two stages)

		let common = self.keygen_common.common.clone();
		let processor = CoefficientCommitments3 {
			keygen_common: self.keygen_common,
			hash_commitments,
			own_commitment: self.own_commitment,
			shares: self.shares_to_send,
		};

		let stage = BroadcastStage::new(processor, common);

		StageResult::NextStage(Box::new(stage))
	}
}

/// Stage 3: Sample a secret, generate sharing polynomial coefficients for it
/// and a ZKP of the secret. Broadcast commitments to the coefficients and the ZKP.
pub struct CoefficientCommitments3<C: CryptoScheme> {
	keygen_common: KeygenCommon<C>,
	hash_commitments: BTreeMap<AuthorityCount, HashComm1>,
	own_commitment: DKGUnverifiedCommitment<C::Point>,
	/// Shares generated by us for other parties (secret)
	shares: OutgoingShares<C::Point>,
}

derive_display_as_type_name!(CoefficientCommitments3<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for CoefficientCommitments3<Crypto>
{
	type Message = CoeffComm3<Crypto::Point>;
	const NAME: KeygenStageName = KeygenStageName::CoefficientCommitments3;

	fn init(&mut self) -> DataToSend<Self::Message> {
		DataToSend::Broadcast(self.own_commitment.clone())
	}

	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> KeygenStageResult<Crypto> {
		// We have received commitments from everyone, for now just need to
		// go through another round to verify consistent broadcasts
		let common = self.keygen_common.common.clone();
		let processor = VerifyCommitmentsBroadcast4 {
			keygen_common: self.keygen_common,
			hash_commitments: self.hash_commitments,
			commitments: messages,
			shares_to_send: self.shares,
		};

		let stage = BroadcastStage::new(processor, common);

		StageResult::NextStage(Box::new(stage))
	}
}

/// Stage 4: verify broadcasts of Stage 3 data
struct VerifyCommitmentsBroadcast4<Crypto: CryptoScheme> {
	keygen_common: KeygenCommon<Crypto>,
	hash_commitments: BTreeMap<AuthorityCount, HashComm1>,
	commitments: BTreeMap<AuthorityCount, Option<CoeffComm3<Crypto::Point>>>,
	shares_to_send: OutgoingShares<Crypto::Point>,
}

derive_display_as_type_name!(VerifyCommitmentsBroadcast4<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for VerifyCommitmentsBroadcast4<Crypto>
{
	type Message = VerifyCoeffComm4<Crypto::Point>;
	const NAME: KeygenStageName = KeygenStageName::VerifyCommitmentsBroadcast4;

	fn init(&mut self) -> DataToSend<Self::Message> {
		let data = self.commitments.clone();

		DataToSend::Broadcast(VerifyCoeffComm4 { data })
	}

	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> KeygenStageResult<Crypto> {
		let commitments = match verify_broadcasts(messages) {
			Ok(comms) => comms,
			Err((reported_parties, abort_reason)) =>
				return KeygenStageResult::Error(
					reported_parties,
					KeygenFailureReason::BroadcastFailure(abort_reason, Self::NAME),
				),
		};

		let KeygenCommon { common, resharing_context, keygen_context, .. } = &self.keygen_common;

		// In the case of key handover, remove data from all non-sharing
		// parties so we don't accidentally use it
		let commitments = if let Some(context) = resharing_context {
			commitments
				.into_iter()
				.filter(|(idx, _)| context.sharing_participants.contains(idx))
				.collect()
		} else {
			commitments
		};

		let commitments = match validate_commitments(
			commitments,
			self.hash_commitments,
			resharing_context.as_ref(),
			keygen_context,
			common.validator_mapping.clone(),
		) {
			Ok(comms) => comms,
			Err((blamed_parties, reason)) => return StageResult::Error(blamed_parties, reason),
		};

		debug!("{} is successful", Self::NAME);

		// At this point we know everyone's commitments, which can already be
		// used to derive the resulting aggregate public key.

		let agg_pubkey = derive_aggregate_pubkey::<Crypto>(&commitments);
		let common = self.keygen_common.common.clone();
		let processor = SecretSharesStage5 {
			keygen_common: self.keygen_common,
			commitments,
			shares: self.shares_to_send,
			agg_pubkey,
		};

		let stage = BroadcastStage::new(processor, common);

		StageResult::NextStage(Box::new(stage))
	}
}

/// Stage 5: distribute (distinct) secret shares of our secret to each party
struct SecretSharesStage5<Crypto: CryptoScheme> {
	keygen_common: KeygenCommon<Crypto>,
	// commitments (verified to have been broadcast correctly)
	commitments: BTreeMap<AuthorityCount, DKGCommitment<Crypto::Point>>,
	shares: OutgoingShares<Crypto::Point>,
	agg_pubkey: ValidAggregateKey<Crypto::Point>,
}

derive_display_as_type_name!(SecretSharesStage5<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for SecretSharesStage5<Crypto>
{
	type Message = SecretShare5<Crypto::Point>;
	const NAME: KeygenStageName = KeygenStageName::SecretSharesStage5;

	fn init(&mut self) -> DataToSend<Self::Message> {
		// With everyone committed to their secrets and sharing polynomial coefficients
		// we can now send the *distinct* secret shares to each party
		// Insert dummy shares for parties who should not receive anything
		// (this makes the reset of the code less complex, as currently all
		// parties expect messages from all other parties)
		for idx in &self.keygen_common.common.all_idxs {
			self.shares.0.entry(*idx).or_insert_with(|| {
				use crate::multisig::crypto::ECScalar;
				ShamirShare { value: <Crypto::Point as ECPoint>::Scalar::zero() }
			});
		}

		DataToSend::Private(self.shares.0.clone())
	}

	async fn process(
		self,
		incoming_shares: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> KeygenStageResult<Crypto> {
		// As the messages for this stage are sent in secret, it is possible
		// for a malicious party to send us invalid data (or not send anything
		// at all) without us being able to prove that. Because of that, we
		// can't simply terminate our protocol here.

		let KeygenCommon { common, resharing_context, .. } = &self.keygen_common;

		let should_process_shares = resharing_context
			.as_ref()
			.map_or(true, |context| context.receiving_participants.contains(&common.own_idx));

		let mut bad_parties = BTreeSet::new();
		let verified_shares = if should_process_shares {
			// Index at which we should evaluate sharing polynomial
			let evaluation_index = if let Some(context) = resharing_context {
				let own_id = common.validator_mapping.get_id(common.own_idx);
				context.future_index_mapping.get_idx(own_id).unwrap()
			} else {
				common.own_idx
			};

			incoming_shares
				.into_iter()
				.filter_map(|(sender_idx, share_opt)| {
					if let Some(context) = resharing_context {
						// Ignore (dummy) shares from non-sharing parties:
						if !context.sharing_participants.contains(&sender_idx) {
							return None
						}

						// Ignore all shares if we are not the recipient:
						if !context.receiving_participants.contains(&common.own_idx) {
							return None
						}
					}

					if let Some(share) = share_opt {
						if verify_share(&share, &self.commitments[&sender_idx], evaluation_index) {
							Some((sender_idx, share))
						} else {
							warn!(
								from_id = common.validator_mapping.get_id(sender_idx).to_string(),
								"Received invalid secret share"
							);

							bad_parties.insert(sender_idx);
							None
						}
					} else {
						warn!(
							from_id = common.validator_mapping.get_id(sender_idx).to_string(),
							"Received no secret share",
						);

						bad_parties.insert(sender_idx);
						None
					}
				})
				.collect()
		} else {
			Default::default()
		};

		let common = self.keygen_common.common.clone();
		let processor = ComplaintsStage6 {
			keygen_common: self.keygen_common,
			commitments: self.commitments,
			agg_pubkey: self.agg_pubkey,
			shares: IncomingShares(verified_shares),
			outgoing_shares: self.shares,
			complaints: bad_parties,
		};
		let stage = BroadcastStage::new(processor, common);

		StageResult::NextStage(Box::new(stage))
	}
}

/// During this stage parties have a chance to complain about
/// a party sending a secret share that isn't valid when checked
/// against the commitments
struct ComplaintsStage6<Crypto: CryptoScheme> {
	keygen_common: KeygenCommon<Crypto>,
	// commitments (verified to have been broadcast correctly)
	commitments: BTreeMap<AuthorityCount, DKGCommitment<Crypto::Point>>,
	agg_pubkey: ValidAggregateKey<Crypto::Point>,
	/// Shares sent to us from other parties (secret)
	shares: IncomingShares<Crypto::Point>,
	outgoing_shares: OutgoingShares<Crypto::Point>,
	complaints: BTreeSet<AuthorityCount>,
}

derive_display_as_type_name!(ComplaintsStage6<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for ComplaintsStage6<Crypto>
{
	type Message = Complaints6;
	const NAME: KeygenStageName = KeygenStageName::ComplaintsStage6;

	fn init(&mut self) -> DataToSend<Self::Message> {
		DataToSend::Broadcast(Complaints6(self.complaints.clone()))
	}

	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> KeygenStageResult<Crypto> {
		let common = self.keygen_common.common.clone();
		let processor = VerifyComplaintsBroadcastStage7 {
			keygen_common: self.keygen_common,
			agg_pubkey: self.agg_pubkey,
			received_complaints: messages,
			commitments: self.commitments,
			shares: self.shares,
			outgoing_shares: self.outgoing_shares,
		};

		let stage = BroadcastStage::new(processor, common);

		StageResult::NextStage(Box::new(stage))
	}
}

struct VerifyComplaintsBroadcastStage7<Crypto: CryptoScheme> {
	keygen_common: KeygenCommon<Crypto>,
	agg_pubkey: ValidAggregateKey<Crypto::Point>,
	received_complaints: BTreeMap<AuthorityCount, Option<Complaints6>>,
	commitments: BTreeMap<AuthorityCount, DKGCommitment<Crypto::Point>>,
	shares: IncomingShares<Crypto::Point>,
	outgoing_shares: OutgoingShares<Crypto::Point>,
}

derive_display_as_type_name!(VerifyComplaintsBroadcastStage7<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for VerifyComplaintsBroadcastStage7<Crypto>
{
	type Message = VerifyComplaints7;
	const NAME: KeygenStageName = KeygenStageName::VerifyComplaintsBroadcastStage7;

	fn init(&mut self) -> DataToSend<Self::Message> {
		let data = self.received_complaints.clone();

		DataToSend::Broadcast(VerifyComplaints7 { data })
	}

	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> KeygenStageResult<Crypto> {
		let verified_complaints = match verify_broadcasts(messages) {
			Ok(comms) => comms,
			Err((reported_parties, abort_reason)) =>
				return KeygenStageResult::Error(
					reported_parties,
					KeygenFailureReason::BroadcastFailure(abort_reason, Self::NAME),
				),
		};

		if verified_complaints.iter().all(|(_idx, c)| c.0.is_empty()) {
			// if all complaints are empty, we can finalize the ceremony
			return finalize_keygen::<Crypto>(
				self.keygen_common,
				self.agg_pubkey,
				self.shares,
				self.commitments,
			)
			.await
		};

		// Some complaints have been issued, entering the blaming stage

		let common = &self.keygen_common.common;

		let idxs_to_report: BTreeSet<_> = verified_complaints
			.iter()
			.filter_map(|(idx_from, Complaints6(blamed_idxs))| {
				let has_invalid_idxs = !blamed_idxs.iter().all(|idx_blamed| {
					if common.is_idx_valid(*idx_blamed) {
						true
					} else {
						warn!(
							from_id = common.validator_mapping.get_id(*idx_from).to_string(),
							"Invalid index [{idx_blamed}] in complaint",
						);
						false
					}
				});

				if has_invalid_idxs {
					Some(*idx_from)
				} else {
					None
				}
			})
			.collect();

		if idxs_to_report.is_empty() {
			let common = self.keygen_common.common.clone();
			let processor = BlameResponsesStage8 {
				keygen_common: self.keygen_common,
				complaints: verified_complaints,
				agg_pubkey: self.agg_pubkey,
				shares: self.shares,
				outgoing_shares: self.outgoing_shares,
				commitments: self.commitments,
			};

			let stage = BroadcastStage::new(processor, common);

			StageResult::NextStage(Box::new(stage))
		} else {
			StageResult::Error(idxs_to_report, KeygenFailureReason::InvalidComplaint)
		}
	}
}

async fn finalize_keygen<Crypto: CryptoScheme>(
	keygen_common: KeygenCommon<Crypto>,
	agg_pubkey: ValidAggregateKey<Crypto::Point>,
	secret_shares: IncomingShares<Crypto::Point>,
	commitments: BTreeMap<AuthorityCount, DKGCommitment<Crypto::Point>>,
) -> StageResult<KeygenCeremony<Crypto>> {
	let future_index_mapping = keygen_common
		.resharing_context
		.map(|c| Arc::new(c.future_index_mapping))
		.unwrap_or_else(|| keygen_common.common.validator_mapping.clone());

	// Making a copy while we still have sharing parameters
	let key_params = keygen_common.sharing_params.key_params;

	let party_public_keys = tokio::task::spawn_blocking(move || {
		derive_local_pubkeys_for_parties(&keygen_common.sharing_params, &commitments)
	})
	.await
	.unwrap();

	// `derive_local_pubkeys_for_parties` returns a vector of public keys where
	// the index corresponds to the party's index in a ceremony. In a key handover
	// ceremony we want to re-arrange them according to party's indices in future
	// signing ceremonies.
	// TODO: it would be nicer if we stored account ids alongside the public key
	// shares (would require a db migration though)
	let party_public_keys = party_public_keys
		.into_iter()
		.map(|(idx, pk)| {
			(
				{
					let id = keygen_common.common.validator_mapping.get_id(idx);
					future_index_mapping
						.get_idx(id)
						.expect("receiving party must have a future index")
				},
				pk,
			)
		})
		.sorted_by_key(|(idx, _)| *idx)
		.map(|(_, pk)| pk)
		.collect();

	let keygen_result_info = KeygenResultInfo {
		key: Arc::new(KeygenResult::new_compatible(
			KeyShare { y: agg_pubkey.0, x_i: compute_secret_key_share(secret_shares) },
			party_public_keys,
		)),
		validator_mapping: future_index_mapping,
		params: key_params,
	};

	StageResult::Done(keygen_result_info)
}

struct BlameResponsesStage8<Crypto: CryptoScheme> {
	keygen_common: KeygenCommon<Crypto>,
	complaints: BTreeMap<AuthorityCount, Complaints6>,
	agg_pubkey: ValidAggregateKey<Crypto::Point>,
	shares: IncomingShares<Crypto::Point>,
	outgoing_shares: OutgoingShares<Crypto::Point>,
	commitments: BTreeMap<AuthorityCount, DKGCommitment<Crypto::Point>>,
}

derive_display_as_type_name!(BlameResponsesStage8<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for BlameResponsesStage8<Crypto>
{
	type Message = BlameResponse8<Crypto::Point>;
	const NAME: KeygenStageName = KeygenStageName::BlameResponsesStage8;

	fn init(&mut self) -> DataToSend<Self::Message> {
		let common = &self.keygen_common.common;

		// Indexes at which to reveal/broadcast secret shares
		let idxs_to_reveal: Vec<_> = self
			.complaints
			.iter()
			.filter_map(|(idx, complaint)| {
				if complaint.0.contains(&common.own_idx) {
					warn!("We are blamed by {}", common.validator_mapping.get_id(*idx).to_string());

					Some(*idx)
				} else {
					None
				}
			})
			.collect();

		// TODO: put a limit on how many shares to reveal?
		let data = DataToSend::Broadcast(BlameResponse8(
			idxs_to_reveal
				.iter()
				.map(|idx| {
					debug!(
						"Revealing share for {}",
						common.validator_mapping.get_id(*idx).to_string()
					);
					(*idx, self.outgoing_shares.0[idx].clone())
				})
				.collect(),
		));

		// Outgoing shares are no longer needed, so we zeroize them
		drop(std::mem::take(&mut self.outgoing_shares));

		data
	}

	async fn process(
		self,
		blame_responses: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> KeygenStageResult<Crypto> {
		let common = self.keygen_common.common.clone();
		let processor = VerifyBlameResponsesBroadcastStage9 {
			keygen_common: self.keygen_common,
			complaints: self.complaints,
			agg_pubkey: self.agg_pubkey,
			blame_responses,
			shares: self.shares,
			commitments: self.commitments,
		};

		let stage = BroadcastStage::new(processor, common);

		StageResult::NextStage(Box::new(stage))
	}
}

struct VerifyBlameResponsesBroadcastStage9<Crypto: CryptoScheme> {
	keygen_common: KeygenCommon<Crypto>,
	complaints: BTreeMap<AuthorityCount, Complaints6>,
	agg_pubkey: ValidAggregateKey<Crypto::Point>,
	// Blame responses received from other parties in the previous communication round
	blame_responses: BTreeMap<AuthorityCount, Option<BlameResponse8<Crypto::Point>>>,
	shares: IncomingShares<Crypto::Point>,
	commitments: BTreeMap<AuthorityCount, DKGCommitment<Crypto::Point>>,
}

derive_display_as_type_name!(VerifyBlameResponsesBroadcastStage9<Crypto: CryptoScheme>);

/// Checks for sender_idx that their blame response contains exactly
/// a share for each party that blamed them
fn is_blame_response_complete<P: ECPoint>(
	sender_idx: AuthorityCount,
	response: &BlameResponse8<P>,
	complaints: &BTreeMap<AuthorityCount, Complaints6>,
) -> bool {
	let expected_idxs_iter = complaints
		.iter()
		.filter_map(
			|(blamer_idx, Complaints6(blamed_idxs))| {
				if blamed_idxs.contains(&sender_idx) {
					Some(blamer_idx)
				} else {
					None
				}
			},
		)
		.sorted();

	// Note: the keys in BTreeMap are already sorted
	Iterator::eq(response.0.keys(), expected_idxs_iter)
}

impl<Crypto: CryptoScheme> VerifyBlameResponsesBroadcastStage9<Crypto> {
	/// Check that blame responses contain all (and only) the requested shares, and that all the
	/// shares are valid. If all responses are valid, returns shares destined for us along with the
	/// corresponding index. Otherwise, returns a list of party indexes who provided invalid
	/// responses.
	fn check_blame_responses(
		&self,
		blame_responses: BTreeMap<AuthorityCount, BlameResponse8<Crypto::Point>>,
	) -> Result<BTreeMap<AuthorityCount, ShamirShare<Crypto::Point>>, BTreeSet<AuthorityCount>> {
		let common = &self.keygen_common.common;
		let (shares_for_us, bad_parties): (Vec<_>, BTreeSet<_>) = blame_responses
			.iter()
			.map(|(sender_idx, response)| {
				if !is_blame_response_complete(*sender_idx, response, &self.complaints) {
					warn!(
						from_id = common.validator_mapping.get_id(*sender_idx).to_string(),
						"Incomplete blame response",
					);

					return Err(sender_idx)
				}

				if !response.0.iter().all(|(dest_idx, share)| {
					verify_share(share, &self.commitments[sender_idx], *dest_idx)
				}) {
					warn!(
						from_id = common.validator_mapping.get_id(*sender_idx).to_string(),
						"Invalid secret share in a blame response"
					);

					return Err(sender_idx)
				}

				Ok((*sender_idx, response.0.get(&common.own_idx)))
			})
			.partition_result();

		if bad_parties.is_empty() {
			let shares_for_us = shares_for_us
				.into_iter()
				.filter_map(|(sender_idx, opt_share)| {
					opt_share.map(|share| (sender_idx, share.clone()))
				})
				.collect();

			Ok(shares_for_us)
		} else {
			Err(bad_parties)
		}
	}
}

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<KeygenCeremony<Crypto>>
	for VerifyBlameResponsesBroadcastStage9<Crypto>
{
	type Message = VerifyBlameResponses9<Crypto::Point>;
	const NAME: KeygenStageName = KeygenStageName::VerifyBlameResponsesBroadcastStage9;

	fn init(&mut self) -> DataToSend<Self::Message> {
		let data = self.blame_responses.clone();

		DataToSend::Broadcast(VerifyBlameResponses9 { data })
	}

	async fn process(
		mut self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> KeygenStageResult<Crypto> {
		debug!("Processing {}", Self::NAME);

		let verified_responses = match verify_broadcasts(messages) {
			Ok(comms) => comms,
			Err((reported_parties, abort_reason)) =>
				return KeygenStageResult::Error(
					reported_parties,
					KeygenFailureReason::BroadcastFailure(abort_reason, Self::NAME),
				),
		};

		match self.check_blame_responses(verified_responses) {
			Ok(shares_for_us) => {
				for (sender_idx, share) in shares_for_us {
					self.shares.0.insert(sender_idx, share);
				}

				finalize_keygen(self.keygen_common, self.agg_pubkey, self.shares, self.commitments)
					.await
			},
			Err(bad_parties) =>
				StageResult::Error(bad_parties, KeygenFailureReason::InvalidBlameResponse),
		}
	}
}
