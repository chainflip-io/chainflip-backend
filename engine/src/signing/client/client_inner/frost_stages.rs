use std::collections::HashMap;

use super::common::{
    broadcast::{BroadcastStage, BroadcastStageProcessor},
    {CeremonyCommon, StageResult},
};
use super::frost::{
    self, BroadcastVerificationMessage, Comm1, LocalSig3, SecretNoncePair, SigningData,
    VerifyComm2, VerifyLocalSig4,
};
use super::SchnorrSignature;

use super::signing_state::{SigningP2PSender, SigningStateCommonInfo};

use super::utils::threshold_from_share_count;

type SigningStageResult = StageResult<SigningData, SchnorrSignature>;

macro_rules! should_delay {
    ($variant:path) => {
        fn should_delay(&self, m: &SigningData) -> bool {
            matches!(m, $variant(_))
        }
    };
}

type SigningCeremonyCommon = CeremonyCommon<SigningData, SigningP2PSender>;

// *********** Await Commitments1 *************

/// Stage 1: Generate a broadcast our (secret, nonce) pair
/// and collect those from all other parties
#[derive(Clone)]
pub struct AwaitCommitments1 {
    common: SigningCeremonyCommon,
    signing_common: SigningStateCommonInfo,
    // TODO: I probably shouldn't make copies/move this as we progress though
    // stages (put in the Box?)
    nonces: SecretNoncePair,
}

impl AwaitCommitments1 {
    pub fn new(common: SigningCeremonyCommon, signing_common: SigningStateCommonInfo) -> Self {
        AwaitCommitments1 {
            common,
            signing_common,
            nonces: SecretNoncePair::sample_random(),
        }
    }
}

derive_display_as_type_name!(AwaitCommitments1);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for AwaitCommitments1 {
    type Message = Comm1;

    fn init(&self) -> Self::Message {
        Comm1 {
            index: self.common.own_idx,
            d: self.nonces.d_pub,
            e: self.nonces.e_pub,
        }
    }

    should_delay!(SigningData::BroadcastVerificationStage2);

    fn process(self, messages: HashMap<usize, Self::Message>) -> SigningStageResult {
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
    common: SigningCeremonyCommon,
    signing_common: SigningStateCommonInfo,
    // Our nonce pair generated in the previous stage
    nonces: SecretNoncePair,
    // Public nonce commitments to be collected
    commitments: HashMap<usize, Comm1>,
}

derive_display_as_type_name!(VerifyCommitmentsBroadcast2);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for VerifyCommitmentsBroadcast2 {
    type Message = VerifyComm2;

    /// Simply report all data that we have received from
    /// other parties in the last stage
    fn init(&self) -> Self::Message {
        let data = self
            .common
            .all_idxs
            .iter()
            .map(|idx| {
                self.commitments
                    .get(&idx)
                    .cloned()
                    .expect("All indexes should be present here")
            })
            .collect();

        VerifyComm2 { data }
    }

    should_delay!(SigningData::LocalSigStage3);

    /// Verify that all values have been broadcast correctly during stage 1
    fn process(self, messages: HashMap<usize, Self::Message>) -> SigningStageResult {
        let verified_commitments = match verify_broadcasts(&self.common.all_idxs, &messages) {
            Ok(comms) => comms,
            Err(blamed_parties) => {
                return StageResult::Error(blamed_parties);
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
    common: SigningCeremonyCommon,
    signing_common: SigningStateCommonInfo,
    // Our nonce pair generated in the previous stage
    nonces: SecretNoncePair,
    // Public nonce commitments (verified)
    commitments: Vec<Comm1>,
}

derive_display_as_type_name!(LocalSigStage3);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for LocalSigStage3 {
    type Message = LocalSig3;

    /// With all nonce commitments verified, we can generate the group commitment
    /// and our share of signature response, which we broadcast to other parties.
    fn init(&self) -> Self::Message {
        slog::trace!(self.common.logger, "Generating local signature response");

        frost::generate_local_sig(
            &self.signing_common.data.0,
            &self.signing_common.key.key_share,
            &self.nonces,
            &self.commitments,
            self.common.own_idx,
            &self.common.all_idxs,
        )

        // TODO: make sure secret nonces are deleted here (according to
        // step 6, Figure 3 in https://eprint.iacr.org/2020/852.pdf).
        // Zeroize memory if needed.
    }

    should_delay!(SigningData::VerifyLocalSigsStage4);

    /// Nothing to process here yet, simply creating the new stage once all of the
    /// data has been collected
    fn process(self, messages: HashMap<usize, Self::Message>) -> SigningStageResult {
        let processor = VerifyLocalSigsBroadcastStage4 {
            common: self.common.clone(),
            signing_common: self.signing_common.clone(),
            commitments: self.commitments,
            sig_shares_received: messages,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

/// Stage 4: Verifying the broadcasting of signature shares
#[derive(Clone)]
struct VerifyLocalSigsBroadcastStage4 {
    common: SigningCeremonyCommon,
    signing_common: SigningStateCommonInfo,
    /// Nonce commitments from all parties (verified to be correctly broadcast)
    commitments: Vec<Comm1>,

    /// Signature shares sent to us (NOT verified to be correctly broadcast)
    sig_shares_received: HashMap<usize, LocalSig3>,
}

derive_display_as_type_name!(VerifyLocalSigsBroadcastStage4);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for VerifyLocalSigsBroadcastStage4 {
    type Message = VerifyLocalSig4;

    /// Broadcast all signature shares sent to us
    fn init(&self) -> Self::Message {
        let data = self
            .common
            .all_idxs
            .iter()
            .map(|idx| {
                self.sig_shares_received
                    .get(&idx)
                    .cloned()
                    .expect("All indexes should be present here")
            })
            .collect();

        VerifyLocalSig4 { data }
    }

    fn should_delay(&self, _: &SigningData) -> bool {
        // Nothing to delay as we don't expect any further stages
        false
    }

    /// Verify that signature shares have been broadcast correctly by? other nodes? us?, and if so,
    /// combine them into the (final) aggregate signature
    fn process(self, messages: HashMap<usize, Self::Message>) -> SigningStageResult {
        let local_sigs = match verify_broadcasts(&self.common.all_idxs, &messages) {
            Ok(sigs) => sigs,
            Err(blamed_parties) => {
                return StageResult::Error(blamed_parties);
            }
        };

        slog::debug!(
            self.common.logger,
            "Local signatures have been correctly broadcast for ceremony: [todo]"
        );

        let all_idxs = &self.common.all_idxs;

        let pubkeys: Vec<_> = all_idxs
            .iter()
            .map(|idx| self.signing_common.key.party_public_keys[idx - 1])
            .collect();

        match frost::aggregate_signature(
            &self.signing_common.data.0,
            &all_idxs,
            self.signing_common.key.get_public_key(),
            &pubkeys,
            &self.commitments,
            &local_sigs,
        ) {
            Ok(sig) => StageResult::Done(sig),
            Err(failed_idxs) => StageResult::Error(failed_idxs),
        }
    }
}

// This might result in an error in case we don't get 2/3 of parties agreeing on the same value.
// If we don't, this means that either the broadcaster did an inconsitent broadcast or that
// 1/3 of parties colluded to slash the broadcasting party. (Should we reduce the threshold to 50%
// for symmetry?)
fn verify_broadcasts<T: Clone + serde::Serialize + serde::de::DeserializeOwned>(
    signer_idxs: &[usize],
    verification_messages: &HashMap<usize, BroadcastVerificationMessage<T>>,
) -> Result<Vec<T>, Vec<usize>> {
    let num_parties = signer_idxs.len();

    // Sanity check: we should have N messages, each containing N messages
    assert_eq!(verification_messages.len(), num_parties);

    assert!(verification_messages
        .iter()
        .all(|(_, m)| m.data.len() == num_parties));

    let threshold = threshold_from_share_count(num_parties);

    // NOTE: ideally we wouldn't need to serialize the messages again here, but
    // we can't use T as key directly (in our case it holds third-party structs)
    // and delaying deserialization when we receive these over p2p would would make
    // our code more complicated than necessary.

    let mut agreed_on_values: Vec<T> = Vec::with_capacity(num_parties);

    let mut blamed_parties = vec![];

    'outer: for i in 0..num_parties {
        let mut value_counts = HashMap::<Vec<u8>, usize>::new();
        for m in verification_messages.values() {
            let data =
                bincode::serialize(&m.data[i]).expect("Could not serialise broadcast message data");
            *value_counts.entry(data).or_default() += 1;
        }

        for (data, count) in value_counts {
            if count > threshold {
                let data = bincode::deserialize::<T>(&data)
                    .expect("Could not deserialise broadcast message data");
                agreed_on_values.push(data);
                continue 'outer;
            }
        }

        // If we reach here, we couldn't reach consensus on
        // values sent from party `idx = i + 1` and we are going to report them
        blamed_parties.push(i + 1);
    }

    if blamed_parties.is_empty() {
        Ok(agreed_on_values)
    } else {
        Err(blamed_parties)
    }
}

#[test]
fn check_correct_broadcast() {
    let mut verification_messages = HashMap::new();

    // There is a concensus on each of the values,
    // even though some parties disagree on some values

    let all_messages = vec![
        vec![1, 1, 1, 1], // "correct" message
        vec![1, 2, 1, 1],
        vec![2, 1, 2, 1],
        vec![1, 1, 1, 2],
    ];

    for (i, m) in all_messages.into_iter().enumerate() {
        verification_messages.insert(i + 1, BroadcastVerificationMessage { data: m });
    }

    assert_eq!(
        verify_broadcasts(&[1, 2, 3, 4], &verification_messages),
        Ok(vec![1, 1, 1, 1])
    );
}

#[test]
fn check_incorrect_broadcast() {
    let mut verification_messages = HashMap::new();

    // We can't achieve consensus on values from parties
    // 2 and 4 (indexes in inner vectors), which we assume
    // is due to them sending messages inconsistently

    let all_messages = vec![
        vec![1, 2, 1, 2],
        vec![1, 2, 1, 1],
        vec![2, 1, 2, 1],
        vec![1, 1, 1, 2],
    ];

    for (i, m) in all_messages.into_iter().enumerate() {
        verification_messages.insert(i + 1, BroadcastVerificationMessage { data: m });
    }

    assert_eq!(
        verify_broadcasts(&[1, 2, 3, 4], &verification_messages),
        Err(vec![2, 4])
    );
}
