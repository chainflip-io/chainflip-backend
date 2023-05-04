use std::collections::{BTreeMap, BTreeSet};

use crate::{
	client::{
		self,
		ceremony_manager::SigningCeremony,
		common::{SigningFailureReason, SigningStageName},
		signing::{self, PayloadAndKey},
	},
	crypto::CryptoScheme,
};

use async_trait::async_trait;
use cf_primitives::AuthorityCount;
use client::common::{
	broadcast::{verify_broadcasts, BroadcastStage, BroadcastStageProcessor, DataToSend},
	CeremonyCommon, StageResult,
};

use signing::signing_detail::{self, SecretNoncePair};

use signing::SigningStateCommonInfo;
use tracing::{debug, warn};

use super::{
	signing_data::{Comm1, LocalSig3, VerifyComm2, VerifyLocalSig4},
	SigningCommitment,
};

type SigningStageResult<Crypto> = StageResult<SigningCeremony<Crypto>>;

// *********** Await Commitments1 *************

/// Stage 1: Generate an broadcast our secret nonce pair
/// and collect those from all other parties
pub struct AwaitCommitments1<Crypto: CryptoScheme> {
	common: CeremonyCommon,
	signing_common: SigningStateCommonInfo<Crypto>,
	// TODO: The reason to keep nonces in a Box was to
	// ensure they are allocated on the heap to avoid leaving
	// copies on the stack when the data is moved. We can probably
	// remove `Box` now that the items are stored in Vec
	nonces: Vec<Box<SecretNoncePair<Crypto::Point>>>,
}

impl<Crypto: CryptoScheme> AwaitCommitments1<Crypto> {
	pub fn new(mut common: CeremonyCommon, signing_common: SigningStateCommonInfo<Crypto>) -> Self {
		let nonces = (0..signing_common.payload_count())
			.map(|_| SecretNoncePair::sample_random(&mut common.rng))
			.collect();

		AwaitCommitments1 { common, signing_common, nonces }
	}
}

derive_display_as_type_name!(AwaitCommitments1<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<SigningCeremony<Crypto>>
	for AwaitCommitments1<Crypto>
{
	type Message = Comm1<Crypto::Point>;
	const NAME: SigningStageName = SigningStageName::AwaitCommitments1;

	fn init(&mut self) -> DataToSend<Self::Message> {
		let comm1 = self
			.nonces
			.iter()
			.map(|nonce| SigningCommitment { d: nonce.d_pub, e: nonce.e_pub })
			.collect();
		DataToSend::Broadcast(Comm1(comm1))
	}

	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> SigningStageResult<Crypto> {
		// No verification is necessary here, just generating new stage

		let processor = VerifyCommitmentsBroadcast2::<Crypto> {
			common: self.common.clone(),
			signing_common: self.signing_common,
			nonces: self.nonces,
			commitments: messages,
		};

		let stage = BroadcastStage::new(processor, self.common);

		StageResult::NextStage(Box::new(stage))
	}
}

// ************

/// Stage 2: Verifying data broadcast during stage 1
struct VerifyCommitmentsBroadcast2<Crypto: CryptoScheme> {
	common: CeremonyCommon,
	signing_common: SigningStateCommonInfo<Crypto>,
	// Our nonce pair generated in the previous stage
	nonces: Vec<Box<SecretNoncePair<Crypto::Point>>>,
	// Public nonce commitments collected in the previous stage
	commitments: BTreeMap<AuthorityCount, Option<Comm1<Crypto::Point>>>,
}

derive_display_as_type_name!(VerifyCommitmentsBroadcast2<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<SigningCeremony<Crypto>>
	for VerifyCommitmentsBroadcast2<Crypto>
{
	type Message = VerifyComm2<Crypto::Point>;
	const NAME: SigningStageName = SigningStageName::VerifyCommitmentsBroadcast2;

	/// Simply report all data that we have received from
	/// other parties in the last stage
	fn init(&mut self) -> DataToSend<Self::Message> {
		let data = self.commitments.clone();

		DataToSend::Broadcast(VerifyComm2 { data })
	}

	/// Verify that all values have been broadcast correctly during stage 1
	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> SigningStageResult<Crypto> {
		let verified_commitments = match verify_broadcasts(messages) {
			Ok(comms) => comms,
			Err((reported_parties, abort_reason)) =>
				return SigningStageResult::Error(
					reported_parties,
					SigningFailureReason::BroadcastFailure(abort_reason, Self::NAME),
				),
		};

		// Check that the number of commitments matches
		// the number of payloads
		// TODO: see if there is a way to deduplicate this
		// (that doesn't add too much complexity)
		let bad_parties: BTreeSet<_> = verified_commitments
			.iter()
			.filter_map(|(party_idx, Comm1(commitments))| {
				if commitments.len() != self.signing_common.payload_count() {
					warn!(
						"Unexpected number of commitments from party {}: {} (expected: {})",
						party_idx,
						commitments.len(),
						self.signing_common.payload_count(),
					);
					Some(*party_idx)
				} else {
					None
				}
			})
			.collect();

		if !bad_parties.is_empty() {
			return SigningStageResult::Error(
				bad_parties,
				SigningFailureReason::InvalidNumberOfPayloads,
			)
		}

		debug!("{} is successful", Self::NAME);

		let processor = LocalSigStage3::<Crypto> {
			common: self.common.clone(),
			signing_common: self.signing_common,
			nonces: self.nonces,
			commitments: verified_commitments,
		};

		let state = BroadcastStage::new(processor, self.common);

		StageResult::NextStage(Box::new(state))
	}
}

/// Stage 3: Generating and broadcasting signature response shares
struct LocalSigStage3<Crypto: CryptoScheme> {
	common: CeremonyCommon,
	signing_common: SigningStateCommonInfo<Crypto>,
	// Our nonce pair generated in the previous stage
	nonces: Vec<Box<SecretNoncePair<Crypto::Point>>>,
	// Public nonce commitments (verified)
	commitments: BTreeMap<AuthorityCount, Comm1<Crypto::Point>>,
}

derive_display_as_type_name!(LocalSigStage3<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<SigningCeremony<Crypto>>
	for LocalSigStage3<Crypto>
{
	type Message = LocalSig3<Crypto::Point>;
	const NAME: SigningStageName = SigningStageName::LocalSigStage3;

	/// With all nonce commitments verified, we can generate the group commitment
	/// and our share of signature response, which we broadcast to other parties.
	fn init(&mut self) -> DataToSend<Self::Message> {
		let responses = (0..self.signing_common.payload_count())
			.map(|i| {
				// Extract commitments for a specific payload (there is some room
				// for optimisation here)
				let commitments = self
					.commitments
					.iter()
					.map(|(party_idx, c)| (*party_idx, c.0[i].clone()))
					.collect();

				let PayloadAndKey { payload, key } = &self.signing_common.payloads_and_keys[i];

				signing_detail::generate_local_sig::<Crypto>(
					payload,
					&key.key_share,
					&self.nonces[i],
					&commitments,
					self.common.own_idx,
					&self.common.all_idxs,
				)
			})
			.collect();

		let data = DataToSend::Broadcast(LocalSig3 { responses });

		use zeroize::Zeroize;

		// Secret nonces are deleted here (according to
		// step 6, Figure 3 in https://eprint.iacr.org/2020/852.pdf).
		for nonce in &mut self.nonces {
			nonce.zeroize();
		}

		data
	}

	/// Nothing to process here yet, simply creating the new stage once all of the
	/// data has been collected
	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> SigningStageResult<Crypto> {
		let processor = VerifyLocalSigsBroadcastStage4::<Crypto> {
			common: self.common.clone(),
			signing_common: self.signing_common,
			commitments: self.commitments,
			local_sigs: messages,
		};

		let stage = BroadcastStage::new(processor, self.common);

		StageResult::NextStage(Box::new(stage))
	}
}

/// Stage 4: Verifying the broadcasting of signature shares
struct VerifyLocalSigsBroadcastStage4<Crypto: CryptoScheme> {
	common: CeremonyCommon,
	signing_common: SigningStateCommonInfo<Crypto>,
	/// Nonce commitments from all parties (verified to be correctly broadcast)
	commitments: BTreeMap<AuthorityCount, Comm1<Crypto::Point>>,
	/// Signature shares sent to us (NOT verified to be correctly broadcast)
	local_sigs: BTreeMap<AuthorityCount, Option<LocalSig3<Crypto::Point>>>,
}

derive_display_as_type_name!(VerifyLocalSigsBroadcastStage4<Crypto: CryptoScheme>);

#[async_trait]
impl<Crypto: CryptoScheme> BroadcastStageProcessor<SigningCeremony<Crypto>>
	for VerifyLocalSigsBroadcastStage4<Crypto>
{
	type Message = VerifyLocalSig4<Crypto::Point>;
	const NAME: SigningStageName = SigningStageName::VerifyLocalSigsBroadcastStage4;

	/// Broadcast all signature shares sent to us
	fn init(&mut self) -> DataToSend<Self::Message> {
		let data = self.local_sigs.clone();

		DataToSend::Broadcast(VerifyLocalSig4 { data })
	}

	/// Verify that signature shares have been broadcast correctly, and if so,
	/// combine them into the (final) aggregate signature
	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> SigningStageResult<Crypto> {
		let local_sigs = match verify_broadcasts(messages) {
			Ok(sigs) => sigs,
			Err((reported_parties, abort_reason)) =>
				return SigningStageResult::Error(
					reported_parties,
					SigningFailureReason::BroadcastFailure(abort_reason, Self::NAME),
				),
		};

		// Check that the number of local signature matches
		// the number of payloads
		let bad_parties: BTreeSet<_> = local_sigs
			.iter()
			.filter_map(|(party_idx, LocalSig3 { responses })| {
				if responses.len() != self.signing_common.payload_count() {
					warn!(
						"Unexpected number of local signatures from party {}: {} (expected: {})",
						party_idx,
						responses.len(),
						self.signing_common.payload_count(),
					);
					Some(*party_idx)
				} else {
					None
				}
			})
			.collect();

		if !bad_parties.is_empty() {
			return SigningStageResult::Error(
				bad_parties,
				SigningFailureReason::InvalidNumberOfPayloads,
			)
		}

		debug!("{} is successful", Self::NAME);

		let all_idxs = &self.common.all_idxs;

		let signatures_result = (0..self.signing_common.payload_count())
			.map(|i| {
				// Extract commitments for a specific payload (there is some room
				// for optimisation here)
				let commitments = self
					.commitments
					.iter()
					.map(|(party_idx, commitments)| (*party_idx, commitments.0[i].clone()))
					.collect();

				// Extract local signatures for a specific payload (there is some
				// room for optimization here)
				let local_sigs = local_sigs
					.iter()
					.map(|(party_idx, local_signatures)| {
						(*party_idx, local_signatures.responses[i].clone())
					})
					.collect();

				let PayloadAndKey { payload, key } = &self.signing_common.payloads_and_keys[i];

				// NOTE: depending on how many payloads we will need to sign with
				// the same key, we may want to compute this value once per key
				let pubkeys: BTreeMap<_, _> = all_idxs
					.iter()
					.map(|idx| (*idx, key.party_public_keys[*idx as usize - 1]))
					.collect();

				signing_detail::aggregate_signature::<Crypto>(
					payload,
					all_idxs,
					key.get_agg_public_key_point(),
					&pubkeys,
					&commitments,
					&local_sigs,
				)
			})
			.collect::<Result<Vec<_>, _>>();

		match signatures_result {
			Ok(signatures) => StageResult::Done(signatures),
			Err(failed_idxs) =>
				StageResult::Error(failed_idxs, SigningFailureReason::InvalidSigShare),
		}
	}
}
