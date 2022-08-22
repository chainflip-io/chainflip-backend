use std::collections::BTreeMap;

use crate::multisig::{
    client::{
        self,
        common::{CeremonyFailureReason, CeremonyStageName, SigningFailureReason},
        signing,
    },
    crypto::CryptoScheme,
};

use async_trait::async_trait;
use cf_traits::AuthorityCount;
use client::common::{
    broadcast::{verify_broadcasts, BroadcastStage, BroadcastStageProcessor, DataToSend},
    {CeremonyCommon, StageResult},
};

use signing::frost::{
    self, Comm1, LocalSig3, SecretNoncePair, SigningData, VerifyComm2, VerifyLocalSig4,
};

use signing::SigningStateCommonInfo;

type SigningStageResult<C> = StageResult<
    SigningData<<C as CryptoScheme>::Point>,
    <C as CryptoScheme>::Signature,
    SigningFailureReason,
>;

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

derive_display_as_type_name!(AwaitCommitments1<C: CryptoScheme>);

#[async_trait]
impl<C: CryptoScheme>
    BroadcastStageProcessor<SigningData<C::Point>, C::Signature, SigningFailureReason>
    for AwaitCommitments1<C>
{
    type Message = Comm1<C::Point>;
    const NAME: CeremonyStageName = CeremonyStageName::AwaitCommitments1;

    fn init(&mut self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(Comm1 {
            d: self.nonces.d_pub,
            e: self.nonces.e_pub,
        })
    }

    async fn process(
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

derive_display_as_type_name!(VerifyCommitmentsBroadcast2<C: CryptoScheme>);

#[async_trait]
impl<C: CryptoScheme>
    BroadcastStageProcessor<SigningData<C::Point>, C::Signature, SigningFailureReason>
    for VerifyCommitmentsBroadcast2<C>
{
    type Message = VerifyComm2<C::Point>;
    const NAME: CeremonyStageName = CeremonyStageName::VerifyCommitmentsBroadcast2;

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
    ) -> SigningStageResult<C> {
        let verified_commitments = match verify_broadcasts(messages, &self.common.logger) {
            Ok(comms) => comms,
            Err((reported_parties, abort_reason)) => {
                return SigningStageResult::<C>::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(abort_reason, Self::NAME),
                );
            }
        };

        slog::debug!(self.common.logger, "{} is successful", Self::NAME);

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

derive_display_as_type_name!(LocalSigStage3<C: CryptoScheme>);

#[async_trait]
impl<C: CryptoScheme>
    BroadcastStageProcessor<SigningData<C::Point>, C::Signature, SigningFailureReason>
    for LocalSigStage3<C>
{
    type Message = LocalSig3<C::Point>;
    const NAME: CeremonyStageName = CeremonyStageName::LocalSigStage3;

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

    /// Nothing to process here yet, simply creating the new stage once all of the
    /// data has been collected
    async fn process(
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

derive_display_as_type_name!(VerifyLocalSigsBroadcastStage4<C: CryptoScheme>);

#[async_trait]
impl<C: CryptoScheme>
    BroadcastStageProcessor<SigningData<C::Point>, C::Signature, SigningFailureReason>
    for VerifyLocalSigsBroadcastStage4<C>
{
    type Message = VerifyLocalSig4<C::Point>;
    const NAME: CeremonyStageName = CeremonyStageName::VerifyLocalSigsBroadcastStage4;

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
    ) -> SigningStageResult<C> {
        let local_sigs = match verify_broadcasts(messages, &self.common.logger) {
            Ok(sigs) => sigs,
            Err((reported_parties, abort_reason)) => {
                return SigningStageResult::<C>::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(abort_reason, Self::NAME),
                );
            }
        };

        slog::debug!(self.common.logger, "{} is successful", Self::NAME);

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
                CeremonyFailureReason::Other(SigningFailureReason::InvalidSigShare),
            ),
        }
    }
}
