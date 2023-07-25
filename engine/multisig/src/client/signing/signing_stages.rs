use std::collections::{BTreeMap, BTreeSet};

use crate::{
	client::{
		self,
		ceremony_manager::SigningCeremony,
		common::{try_deserialize, DelayDeserialization, SigningFailureReason, SigningStageName},
		signing::{self, signing_data::LocalSig3Inner, PayloadAndKey},
	},
	crypto::CryptoScheme,
	ChainSigning,
};

use async_trait::async_trait;
use cf_primitives::AuthorityCount;
use client::common::{
	broadcast::{
		verify_broadcasts_non_blocking, BroadcastStage, BroadcastStageProcessor, DataToSend,
	},
	CeremonyCommon, StageResult,
};

use signing::signing_detail::{self, SecretNoncePair};

use signing::SigningStateCommonInfo;
use signing_detail::get_lagrange_coeff;
use tracing::{debug, warn};

use super::{
	signing_data::{Comm1, LocalSig3, VerifyComm2, VerifyLocalSig4},
	signing_detail::{NonceBinding, SchnorrCommitment},
	SigningCommitment,
};

type SigningStageResult<Chain> = StageResult<SigningCeremony<Chain>>;

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
impl<Chain: ChainSigning> BroadcastStageProcessor<SigningCeremony<Chain>>
	for AwaitCommitments1<Chain::CryptoScheme>
{
	type Message = Comm1<<Chain::CryptoScheme as CryptoScheme>::Point>;
	const NAME: SigningStageName = SigningStageName::AwaitCommitments1;

	fn init(&mut self) -> DataToSend<Self::Message> {
		let comm1: Vec<_> = self
			.nonces
			.iter()
			.map(|nonce| SigningCommitment::<<Chain::CryptoScheme as CryptoScheme>::Point> {
				d: nonce.d_pub,
				e: nonce.e_pub,
			})
			.collect();
		DataToSend::Broadcast(DelayDeserialization::new(&comm1))
	}

	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> SigningStageResult<Chain> {
		// No verification is necessary here, just generating new stage

		let processor = VerifyCommitmentsBroadcast2::<Chain::CryptoScheme> {
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

/// Data derived for a single payload from initial commitments
pub struct DerivedSignatureData<C: CryptoScheme> {
	group_commitment: SchnorrCommitment<C>,
	bindings: BTreeMap<AuthorityCount, NonceBinding<C>>,
	bound_commitments: BTreeMap<AuthorityCount, SchnorrCommitment<C>>,
}

#[async_trait]
impl<Chain: ChainSigning> BroadcastStageProcessor<SigningCeremony<Chain>>
	for VerifyCommitmentsBroadcast2<Chain::CryptoScheme>
{
	type Message = VerifyComm2<<Chain::CryptoScheme as CryptoScheme>::Point>;
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
	) -> SigningStageResult<Chain> {
		let verified_commitments = match verify_broadcasts_non_blocking(messages).await {
			Ok(comms) => comms,
			Err((reported_parties, abort_reason)) =>
				return SigningStageResult::Error(
					reported_parties,
					SigningFailureReason::BroadcastFailure(
						abort_reason,
						<Self as BroadcastStageProcessor<SigningCeremony<Chain>>>::NAME,
					),
				),
		};

		// Deserialize and report any party for which deserialization fails:
		let verified_commitments = match try_deserialize(verified_commitments) {
			Ok(res) => res,
			Err(bad_parties) =>
				return SigningStageResult::Error(
					bad_parties,
					SigningFailureReason::DeserializationError,
				),
		};

		// Check that the number of commitments matches
		// the number of payloads
		// TODO: see if there is a way to deduplicate this
		// (that doesn't add too much complexity)
		let bad_parties: BTreeSet<_> = verified_commitments
			.iter()
			.filter_map(|(party_idx, commitments)| {
				if commitments.0.len() != self.signing_common.payload_count() {
					warn!(
						from_id = self.common.validator_mapping.get_id(*party_idx).to_string(),
						"Unexpected number of commitments from party: {} (expected: {})",
						commitments.0.len(),
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

		debug!("{} is successful", <Self as BroadcastStageProcessor<SigningCeremony<Chain>>>::NAME);

		let signature_data = self
			.signing_common
			.payloads_and_keys
			.iter()
			.enumerate()
			.map(|(payload_idx, PayloadAndKey { payload, .. })| {
				let commitments = verified_commitments
					.iter()
					.map(|(party_idx, commitments)| {
						(*party_idx, commitments.0[payload_idx].clone())
					})
					.collect::<BTreeMap<_, _>>();

				let bindings = signing_detail::generate_bindings::<Chain::CryptoScheme>(
					payload,
					&commitments,
					&self.common.all_idxs,
				);

				let bound_commitments = commitments
					.iter()
					.map(|(idx, comm)| (*idx, comm.d + comm.e * bindings[idx].clone()))
					.collect::<BTreeMap<_, _>>();

				// Combine individual commitments into group (schnorr) commitment.
				// See "Signing Protocol" in Section 5.2 (page 14).
				let group_commitment = bound_commitments.values().cloned().sum();

				DerivedSignatureData { group_commitment, bindings, bound_commitments }
			})
			.collect();

		let processor = LocalSigStage3::<Chain::CryptoScheme> {
			common: self.common.clone(),
			signing_common: self.signing_common,
			nonces: self.nonces,
			signature_data,
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
	signature_data: Vec<DerivedSignatureData<Crypto>>,
}

derive_display_as_type_name!(LocalSigStage3<Crypto: CryptoScheme>);

#[async_trait]
impl<Chain: ChainSigning> BroadcastStageProcessor<SigningCeremony<Chain>>
	for LocalSigStage3<Chain::CryptoScheme>
{
	type Message = LocalSig3<<Chain::CryptoScheme as CryptoScheme>::Point>;
	const NAME: SigningStageName = SigningStageName::LocalSigStage3;

	/// With all nonce commitments verified, and the group commitment computed,
	/// we can generate our share of signature response, which we broadcast to other parties.
	fn init(&mut self) -> DataToSend<Self::Message> {
		let responses = (0..self.signing_common.payload_count())
			.map(|i| {
				let PayloadAndKey { payload, key } = &self.signing_common.payloads_and_keys[i];
				let signature_data = &self.signature_data[i];

				signing_detail::generate_local_sig::<Chain::CryptoScheme>(
					payload,
					&key.key_share,
					&self.nonces[i],
					&signature_data.bindings,
					signature_data.group_commitment,
					self.common.own_idx,
					&self.common.all_idxs,
				)
			})
			.collect();

		let data = DataToSend::Broadcast(DelayDeserialization::new(&LocalSig3Inner::<
			<Chain::CryptoScheme as CryptoScheme>::Point,
		> {
			responses,
		}));

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
	) -> SigningStageResult<Chain> {
		let processor = VerifyLocalSigsBroadcastStage4::<Chain::CryptoScheme> {
			common: self.common.clone(),
			signing_common: self.signing_common,
			signature_data: self.signature_data,
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
	signature_data: Vec<DerivedSignatureData<Crypto>>,
	/// Signature shares sent to us (NOT verified to be correctly broadcast)
	local_sigs: BTreeMap<AuthorityCount, Option<LocalSig3<Crypto::Point>>>,
}

derive_display_as_type_name!(VerifyLocalSigsBroadcastStage4<Crypto: CryptoScheme>);

#[async_trait]
impl<Chain: ChainSigning> BroadcastStageProcessor<SigningCeremony<Chain>>
	for VerifyLocalSigsBroadcastStage4<Chain::CryptoScheme>
{
	type Message = VerifyLocalSig4<<Chain::CryptoScheme as CryptoScheme>::Point>;
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
	) -> SigningStageResult<Chain> {
		let local_sigs = match verify_broadcasts_non_blocking(messages).await {
			Ok(sigs) => sigs,
			Err((reported_parties, abort_reason)) =>
				return SigningStageResult::Error(
					reported_parties,
					SigningFailureReason::BroadcastFailure(
						abort_reason,
						<Self as BroadcastStageProcessor<SigningCeremony<Chain>>>::NAME,
					),
				),
		};

		// Deserialize and report any party for which deserialization fails:
		let local_sigs = match try_deserialize(local_sigs) {
			Ok(res) => res,
			Err(bad_parties) =>
				return SigningStageResult::Error(
					bad_parties,
					SigningFailureReason::DeserializationError,
				),
		};

		// Check that the number of local signature matches
		// the number of payloads
		let bad_parties: BTreeSet<_> = local_sigs
			.iter()
			.filter_map(|(party_idx, LocalSig3Inner { responses })| {
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

		debug!("{} is successful", <Self as BroadcastStageProcessor<SigningCeremony<Chain>>>::NAME);

		let all_idxs = &self.common.all_idxs;

		let lagrange_coefficients: BTreeMap<_, _> = all_idxs
			.iter()
			.map(|signer_idx| {
				(
					*signer_idx,
					get_lagrange_coeff::<<Chain::CryptoScheme as CryptoScheme>::Point>(
						*signer_idx,
						all_idxs,
					),
				)
			})
			.collect();

		let signatures_result = (0..self.signing_common.payload_count())
			.map(|i| {
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
					.map(|idx| {
						(
							*idx,
							*key.party_public_keys
								.get(self.common.validator_mapping.get_id(*idx))
								.expect("should have a public key for this party"),
						)
					})
					.collect();

				let payload_data = &self.signature_data[i];

				signing_detail::aggregate_signature::<Chain::CryptoScheme>(
					payload,
					all_idxs,
					key.get_agg_public_key_point(),
					&pubkeys,
					payload_data.group_commitment,
					&payload_data.bound_commitments,
					&local_sigs,
					&lagrange_coefficients,
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

#[cfg(test)]
mod tests {
	use super::*;

	use crate::{
		bitcoin::{BtcCryptoScheme, BtcSigning},
		client::{
			signing::{gen_signing_data_stage2, SigningData},
			PartyIdxMapping,
		},
		crypto::Rng,
	};
	use cf_primitives::AccountId;
	use rand::SeedableRng;
	use signing::gen_signing_data_stage4;
	use std::{sync::Arc, vec};

	#[tokio::test]
	async fn should_report_on_invalid_number_of_commitments() {
		// Just one participant for this test, so we don't get an inconsistency error
		let participants: BTreeSet<AccountId> = BTreeSet::from_iter([AccountId::from([0; 32])]);
		const OWN_IDX: AuthorityCount = 1;
		// For this test we will have 2 signing payloads
		const NUMBER_OF_PAYLOADS: usize = 2;

		let common = CeremonyCommon {
			own_idx: OWN_IDX,
			outgoing_p2p_message_sender: tokio::sync::mpsc::unbounded_channel().0,
			number_of_signing_payloads: Some(NUMBER_OF_PAYLOADS),
			ceremony_id: Default::default(),
			validator_mapping: Arc::new(PartyIdxMapping::from_participants(participants)),
			all_idxs: BTreeSet::new(),
			rng: Rng::from_seed([0; 32]),
		};

		// Create the dummy stage 2 with the common data
		let stage: VerifyCommitmentsBroadcast2<BtcCryptoScheme> = VerifyCommitmentsBroadcast2 {
			common,
			signing_common: SigningStateCommonInfo { payloads_and_keys: vec![] },
			nonces: vec![],
			commitments: BTreeMap::new(),
		};

		// Generate stage 2 data with too many commitments (3)
		if let SigningData::<<BtcCryptoScheme as CryptoScheme>::Point>::BroadcastVerificationStage2(bv) =
			gen_signing_data_stage2(1, NUMBER_OF_PAYLOADS + 1)
		{
			let messages = BTreeMap::from_iter([(OWN_IDX, Some(bv))]);
			// Process the message and check that we get the correct error
			if let SigningStageResult::<BtcSigning>::Error(blamed, reason) = stage.process(messages).await {
				assert_eq!(reason, SigningFailureReason::InvalidNumberOfPayloads);
				assert_eq!(blamed, BTreeSet::from_iter([OWN_IDX]));
			} else {
				panic!("Unexpected SigningStageResult");
			};
		} else {
			panic!("Expected a SigningData::BroadcastVerificationStage2");
		}
	}

	#[tokio::test]
	async fn should_report_on_invalid_number_of_local_signatures() {
		// Just one participant for this test, so we don't get an inconsistency error
		let participants: BTreeSet<AccountId> = BTreeSet::from_iter([AccountId::from([0; 32])]);
		const OWN_IDX: AuthorityCount = 1;
		// For this test we will have 2 signing payloads
		const NUMBER_OF_PAYLOADS: usize = 2;

		let common = CeremonyCommon {
			own_idx: OWN_IDX,
			outgoing_p2p_message_sender: tokio::sync::mpsc::unbounded_channel().0,
			// Set the number of payloads to 2
			number_of_signing_payloads: Some(NUMBER_OF_PAYLOADS),
			ceremony_id: Default::default(),
			validator_mapping: Arc::new(PartyIdxMapping::from_participants(participants)),
			all_idxs: BTreeSet::new(),
			rng: Rng::from_seed([0; 32]),
		};

		// Create the dummy stage 4 with the common data
		let stage: VerifyLocalSigsBroadcastStage4<BtcCryptoScheme> =
			VerifyLocalSigsBroadcastStage4 {
				common,
				signing_common: SigningStateCommonInfo { payloads_and_keys: vec![] },
				signature_data: vec![],
				local_sigs: BTreeMap::new(),
			};

		// Generate stage 4 data with too many local sigs (3)
		if let SigningData::<<BtcCryptoScheme as CryptoScheme>::Point>::VerifyLocalSigsStage4(bv) =
			gen_signing_data_stage4(1, NUMBER_OF_PAYLOADS + 1)
		{
			let messages = BTreeMap::from_iter([(OWN_IDX, Some(bv))]);
			// Process the message and check that we get the correct error
			if let SigningStageResult::<BtcSigning>::Error(blamed, reason) =
				stage.process(messages).await
			{
				assert_eq!(reason, SigningFailureReason::InvalidNumberOfPayloads);
				// For this test we report our own index because we are the only participant. This
				// does not matter, the code is the same.
				assert_eq!(blamed, BTreeSet::from_iter([OWN_IDX]));
			} else {
				panic!("Unexpected SigningStageResult");
			};
		} else {
			panic!("Expected a SigningData::BroadcastVerificationStage2");
		}
	}
}
