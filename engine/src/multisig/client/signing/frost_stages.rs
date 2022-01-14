use std::collections::HashMap;

use crate::multisig::client::{self, signing};

use client::common::{
    broadcast::{verify_broadcasts, BroadcastStage, BroadcastStageProcessor, DataToSend},
    {CeremonyCommon, StageResult},
};
use client::SchnorrSignature;
use signing::frost::{
    self, Comm1, LocalSig3, SecretNoncePair, SigningData, VerifyComm2, VerifyLocalSig4,
};

use signing::SigningStateCommonInfo;

type SigningStageResult = StageResult<SigningData, SchnorrSignature>;

macro_rules! should_delay {
    ($variant:path) => {
        fn should_delay(&self, m: &SigningData) -> bool {
            matches!(m, $variant(_))
        }
    };
}

// *********** Await Commitments1 *************

/// Stage 1: Generate an broadcast our secret nonce pair
/// and collect those from all other parties
#[derive(Clone)]
pub struct AwaitCommitments1 {
    common: CeremonyCommon,
    signing_common: SigningStateCommonInfo,
    nonces: Box<SecretNoncePair>,
}

impl AwaitCommitments1 {
    pub fn new(mut common: CeremonyCommon, signing_common: SigningStateCommonInfo) -> Self {
        let nonces = SecretNoncePair::sample_random(&mut common.rng);

        AwaitCommitments1 {
            common,
            signing_common,
            nonces,
        }
    }
}

derive_display_as_type_name!(AwaitCommitments1);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for AwaitCommitments1 {
    type Message = Comm1;

    fn init(&mut self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(Comm1 {
            index: self.common.own_idx,
            d: self.nonces.d_pub,
            e: self.nonces.e_pub,
        })
    }

    should_delay!(SigningData::BroadcastVerificationStage2);

    fn process(self, messages: HashMap<usize, Option<Self::Message>>) -> SigningStageResult {
        // No verification is necessary here, just generating new stage

        let processor = VerifyCommitmentsBroadcast2 {
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
#[derive(Clone)]
struct VerifyCommitmentsBroadcast2 {
    common: CeremonyCommon,
    signing_common: SigningStateCommonInfo,
    // Our nonce pair generated in the previous stage
    nonces: Box<SecretNoncePair>,
    // Public nonce commitments collected in the previous stage
    commitments: HashMap<usize, Option<Comm1>>,
}

derive_display_as_type_name!(VerifyCommitmentsBroadcast2);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for VerifyCommitmentsBroadcast2 {
    type Message = VerifyComm2;

    /// Simply report all data that we have received from
    /// other parties in the last stage
    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.commitments.clone();

        DataToSend::Broadcast(VerifyComm2 { data })
    }

    should_delay!(SigningData::LocalSigStage3);

    /// Verify that all values have been broadcast correctly during stage 1
    fn process(self, messages: HashMap<usize, Option<Self::Message>>) -> SigningStageResult {
        let verified_commitments = match verify_broadcasts(messages) {
            Ok(comms) => comms,
            Err(blamed_parties) => {
                return StageResult::Error(
                    blamed_parties,
                    anyhow::Error::msg("Inconsistent broadcast of initial commitments"),
                );
            }
        };

        slog::debug!(
            self.common.logger,
            "Initial commitments have been correctly broadcast"
        );

        let processor = LocalSigStage3 {
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
#[derive(Clone)]
struct LocalSigStage3 {
    common: CeremonyCommon,
    signing_common: SigningStateCommonInfo,
    // Our nonce pair generated in the previous stage
    nonces: Box<SecretNoncePair>,
    // Public nonce commitments (verified)
    commitments: HashMap<usize, Comm1>,
}

derive_display_as_type_name!(LocalSigStage3);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for LocalSigStage3 {
    type Message = LocalSig3;

    /// With all nonce commitments verified, we can generate the group commitment
    /// and our share of signature response, which we broadcast to other parties.
    fn init(&mut self) -> DataToSend<Self::Message> {
        slog::trace!(self.common.logger, "Generating local signature response");

        let data = DataToSend::Broadcast(frost::generate_local_sig(
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

    should_delay!(SigningData::VerifyLocalSigsStage4);

    /// Nothing to process here yet, simply creating the new stage once all of the
    /// data has been collected
    fn process(self, messages: HashMap<usize, Option<Self::Message>>) -> SigningStageResult {
        let processor = VerifyLocalSigsBroadcastStage4 {
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
#[derive(Clone)]
struct VerifyLocalSigsBroadcastStage4 {
    common: CeremonyCommon,
    signing_common: SigningStateCommonInfo,
    /// Nonce commitments from all parties (verified to be correctly broadcast)
    commitments: HashMap<usize, Comm1>,
    /// Signature shares sent to us (NOT verified to be correctly broadcast)
    local_sigs: HashMap<usize, Option<LocalSig3>>,
}

derive_display_as_type_name!(VerifyLocalSigsBroadcastStage4);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for VerifyLocalSigsBroadcastStage4 {
    type Message = VerifyLocalSig4;

    /// Broadcast all signature shares sent to us
    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.local_sigs.clone();

        DataToSend::Broadcast(VerifyLocalSig4 { data })
    }

    fn should_delay(&self, _: &SigningData) -> bool {
        // Nothing to delay as we don't expect any further stages
        false
    }

    /// Verify that signature shares have been broadcast correctly, and if so,
    /// combine them into the (final) aggregate signature
    fn process(self, messages: HashMap<usize, Option<Self::Message>>) -> SigningStageResult {
        let local_sigs = match verify_broadcasts(messages) {
            Ok(sigs) => sigs,
            Err(blamed_parties) => {
                return StageResult::Error(
                    blamed_parties,
                    anyhow::Error::msg("Inconsistent broadcast of local signatures"),
                );
            }
        };

        slog::debug!(
            self.common.logger,
            "Local signatures have been correctly broadcast"
        );

        let all_idxs = &self.common.all_idxs;

        let pubkeys: HashMap<_, _> = all_idxs
            .iter()
            .map(|idx| (*idx, self.signing_common.key.party_public_keys[idx - 1]))
            .collect();

        match frost::aggregate_signature(
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
