use std::{collections::BTreeMap, fmt::Display};

use crate::multisig::{
    client::{self, signing},
    crypto::CryptoScheme,
};

use cf_traits::AuthorityCount;
use client::common::{
    broadcast::{verify_broadcasts, BroadcastStage, BroadcastStageProcessor, DataToSend},
    {CeremonyCommon, StageResult},
};

use signing::frost::{
    self, Comm1, LocalSig3, SecretNoncePair, SigningData, VerifyComm2, VerifyLocalSig4,
};

use signing::SigningStateCommonInfo;

type SigningStageResult<C> =
    StageResult<SigningData<<C as CryptoScheme>::Point>, <C as CryptoScheme>::Signature>;

macro_rules! should_delay {
    ($variant:path) => {
        fn should_delay(&self, m: &SigningData<C::Point>) -> bool {
            matches!(m, $variant(_))
        }
    };
}

// *********** Await Commitments1 *************

/// Stage 1: Generate an broadcast our secret nonce pair
/// and collect those from all other parties
pub struct AwaitCommitments1<C: CryptoScheme> {
    common: CeremonyCommon,
    signing_common: SigningStateCommonInfo<C::Point>,
    nonces: Box<SecretNoncePair<C::Point>>,
}

impl<C: CryptoScheme> AwaitCommitments1<C> {
    pub fn new(
        mut common: CeremonyCommon,
        signing_common: SigningStateCommonInfo<C::Point>,
    ) -> Self {
        let nonces = SecretNoncePair::sample_random(&mut common.rng);

        AwaitCommitments1 {
            common,
            signing_common,
            nonces,
        }
    }
}

impl<C: CryptoScheme> Display for AwaitCommitments1<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AwaitCommitments1")
    }
}

impl<C: CryptoScheme> BroadcastStageProcessor<SigningData<C::Point>, C::Signature>
    for AwaitCommitments1<C>
{
    type Message = Comm1<C::Point>;

    fn init(&mut self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(Comm1 {
            d: self.nonces.d_pub,
            e: self.nonces.e_pub,
        })
    }

    should_delay!(SigningData::BroadcastVerificationStage2<C::Point>);

    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> SigningStageResult<C> {
        // No verification is necessary here, just generating new stage

        let processor = VerifyCommitmentsBroadcast2::<C> {
            common: self.common.clone(),
            signing_common: self.signing_common.clone(),
            nonces: self.nonces,
            commitments: messages,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

// ************

/// Stage 2: Verifying data broadcast during stage 1
struct VerifyCommitmentsBroadcast2<C: CryptoScheme> {
    common: CeremonyCommon,
    signing_common: SigningStateCommonInfo<C::Point>,
    // Our nonce pair generated in the previous stage
    nonces: Box<SecretNoncePair<C::Point>>,
    // Public nonce commitments collected in the previous stage
    commitments: BTreeMap<AuthorityCount, Option<Comm1<C::Point>>>,
}

impl<C: CryptoScheme> Display for VerifyCommitmentsBroadcast2<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VerifyCommitmentsBroadcast2")
    }
}

impl<C: CryptoScheme> BroadcastStageProcessor<SigningData<C::Point>, C::Signature>
    for VerifyCommitmentsBroadcast2<C>
{
    type Message = VerifyComm2<C::Point>;

    /// Simply report all data that we have received from
    /// other parties in the last stage
    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.commitments.clone();

        DataToSend::Broadcast(VerifyComm2 { data })
    }

    should_delay!(SigningData::LocalSigStage3<C::Point>);

    /// Verify that all values have been broadcast correctly during stage 1
    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> SigningStageResult<C> {
        let verified_commitments = match verify_broadcasts(messages, &self.common.logger) {
            Ok(comms) => comms,
            Err(abort_reason) => {
                return abort_reason.into_stage_result_error("initial commitments");
            }
        };

        slog::debug!(
            self.common.logger,
            "Initial commitments have been correctly broadcast"
        );

        let processor = LocalSigStage3::<C> {
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
struct LocalSigStage3<C: CryptoScheme> {
    common: CeremonyCommon,
    signing_common: SigningStateCommonInfo<C::Point>,
    // Our nonce pair generated in the previous stage
    nonces: Box<SecretNoncePair<C::Point>>,
    // Public nonce commitments (verified)
    commitments: BTreeMap<AuthorityCount, Comm1<C::Point>>,
}

impl<C: CryptoScheme> Display for LocalSigStage3<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalSigStage3")
    }
}

impl<C: CryptoScheme> BroadcastStageProcessor<SigningData<C::Point>, C::Signature>
    for LocalSigStage3<C>
{
    type Message = LocalSig3<C::Point>;

    /// With all nonce commitments verified, we can generate the group commitment
    /// and our share of signature response, which we broadcast to other parties.
    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = DataToSend::Broadcast(frost::generate_local_sig::<C>(
            &self.signing_common.data.0,
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

    should_delay!(SigningData::VerifyLocalSigsStage4<C::Point>);

    /// Nothing to process here yet, simply creating the new stage once all of the
    /// data has been collected
    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> SigningStageResult<C> {
        let processor = VerifyLocalSigsBroadcastStage4::<C> {
            common: self.common.clone(),
            signing_common: self.signing_common.clone(),
            commitments: self.commitments,
            local_sigs: messages,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

/// Stage 4: Verifying the broadcasting of signature shares
struct VerifyLocalSigsBroadcastStage4<C: CryptoScheme> {
    common: CeremonyCommon,
    signing_common: SigningStateCommonInfo<C::Point>,
    /// Nonce commitments from all parties (verified to be correctly broadcast)
    commitments: BTreeMap<AuthorityCount, Comm1<C::Point>>,
    /// Signature shares sent to us (NOT verified to be correctly broadcast)
    local_sigs: BTreeMap<AuthorityCount, Option<LocalSig3<C::Point>>>,
}

impl<C: CryptoScheme> Display for VerifyLocalSigsBroadcastStage4<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VerifyLocalSigsBroadcastStage4")
    }
}

impl<C: CryptoScheme> BroadcastStageProcessor<SigningData<C::Point>, C::Signature>
    for VerifyLocalSigsBroadcastStage4<C>
{
    type Message = VerifyLocalSig4<C::Point>;

    /// Broadcast all signature shares sent to us
    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.local_sigs.clone();

        DataToSend::Broadcast(VerifyLocalSig4 { data })
    }

    fn should_delay(&self, _: &SigningData<C::Point>) -> bool {
        // Nothing to delay as we don't expect any further stages
        false
    }

    /// Verify that signature shares have been broadcast correctly, and if so,
    /// combine them into the (final) aggregate signature
    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> SigningStageResult<C> {
        let local_sigs = match verify_broadcasts(messages, &self.common.logger) {
            Ok(sigs) => sigs,
            Err(abort_reason) => {
                return abort_reason.into_stage_result_error("local signatures");
            }
        };

        slog::debug!(
            self.common.logger,
            "Local signatures have been correctly broadcast"
        );

        let all_idxs = &self.common.all_idxs;

        let pubkeys: BTreeMap<_, _> = all_idxs
            .iter()
            .map(|idx| {
                (
                    *idx,
                    self.signing_common.key.party_public_keys[*idx as usize - 1],
                )
            })
            .collect();

        match frost::aggregate_signature::<C>(
            &self.signing_common.data.0,
            all_idxs,
            self.signing_common.key.get_public_key(),
            &pubkeys,
            &self.commitments,
            &local_sigs,
        ) {
            Ok(sig) => StageResult::Done(sig),
            Err(failed_idxs) => StageResult::Error(
                failed_idxs,
                anyhow::Error::msg("Failed to aggregate signature"),
            ),
        }
    }
}
