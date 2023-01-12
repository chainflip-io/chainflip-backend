use std::collections::BTreeMap;

use crate::multisig::{
	client::{
		self,
		ceremony_manager::SigningCeremony,
		common::{SigningFailureReason, SigningStageName},
		signing,
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

use super::signing_data::{Comm1, LocalSig3, VerifyComm2, VerifyLocalSig4};

type SigningStageResult<Crypto> = StageResult<SigningCeremony<Crypto>>;

// *********** Await Commitments1 *************

/// Stage 1: Generate an broadcast our secret nonce pair
/// and collect those from all other parties
pub struct AwaitCommitments1<Crypto: CryptoScheme> {
	common: CeremonyCommon,
	signing_common: SigningStateCommonInfo<Crypto>,
	nonces: Box<SecretNoncePair<Crypto::Point>>,
}

impl<Crypto: CryptoScheme> AwaitCommitments1<Crypto> {
	pub fn new(mut common: CeremonyCommon, signing_common: SigningStateCommonInfo<Crypto>) -> Self {
		let nonces = SecretNoncePair::sample_random(&mut common.rng);

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
		DataToSend::Broadcast(Comm1 { d: self.nonces.d_pub, e: self.nonces.e_pub })
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
	nonces: Box<SecretNoncePair<Crypto::Point>>,
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
		let verified_commitments = match verify_broadcasts(messages, &self.common.logger) {
			Ok(comms) => comms,
			Err((reported_parties, abort_reason)) =>
				return SigningStageResult::Error(
					reported_parties,
					SigningFailureReason::BroadcastFailure(abort_reason, Self::NAME),
				),
		};

		slog::debug!(self.common.logger, "{} is successful", Self::NAME);

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
	nonces: Box<SecretNoncePair<Crypto::Point>>,
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
		let data = DataToSend::Broadcast(signing_detail::generate_local_sig::<Crypto>(
			&self.signing_common.payload,
			&self.signing_common.key.key_share,
			&self.nonces,
			&self.commitments,
			self.common.own_idx,
			&self.common.all_idxs,
		));

		use zeroize::Zeroize;

		// Secret nonces are deleted here (according to
		// step 6, Figure 3 in https://eprint.iacr.org/2020/852.pdf).
		self.nonces.zeroize();

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
		let local_sigs = match verify_broadcasts(messages, &self.common.logger) {
			Ok(sigs) => sigs,
			Err((reported_parties, abort_reason)) =>
				return SigningStageResult::Error(
					reported_parties,
					SigningFailureReason::BroadcastFailure(abort_reason, Self::NAME),
				),
		};

		slog::debug!(self.common.logger, "{} is successful", Self::NAME);

		let all_idxs = &self.common.all_idxs;

		let pubkeys: BTreeMap<_, _> = all_idxs
			.iter()
			.map(|idx| (*idx, self.signing_common.key.party_public_keys[*idx as usize - 1]))
			.collect();

		match signing_detail::aggregate_signature::<Crypto>(
			&self.signing_common.payload,
			all_idxs,
			self.signing_common.key.get_public_key(),
			&pubkeys,
			&self.commitments,
			&local_sigs,
		) {
			Ok(sig) => StageResult::Done(sig),
			Err(failed_idxs) =>
				StageResult::Error(failed_idxs, SigningFailureReason::InvalidSigShare),
		}
	}
}
