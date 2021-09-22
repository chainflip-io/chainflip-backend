use std::collections::HashMap;

use super::common::broadcast::{BroadcastStage, BroadcastStageProcessor};
use super::frost::{
    self, BroadcastVerificationMessage, Comm1, LocalSig3, SecretNoncePair, SigningData,
    VerifyComm2, VerifyLocalSig4,
};
use super::SchnorrSignature;

use super::signing_state::{SigningP2PSender, SigningStateCommonInfo};

use super::common::{CeremonyCommon, StageResult};

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

#[derive(Clone)]
pub struct AwaitCommitments1 {
    signing_common: SigningStateCommonInfo,
    common: SigningCeremonyCommon,
    // I probably shouldn't make copies/move this as we progress though
    // stages
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

derive_display!(AwaitCommitments1);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for AwaitCommitments1 {
    type Message = Comm1;

    fn init(&self) -> Self::Message {
        Comm1 {
            index: self.common.own_idx,
            d: self.nonces.d_pub,
            e: self.nonces.e_pub,
        }
        .into()
    }

    should_delay!(SigningData::BroadcastVerificationStage2);

    fn process(self, messages: HashMap<usize, Self::Message>) -> SigningStageResult {
        // No verfication is necessary here, just generating new stage

        let processor = VerifyCommitmentsBroadcast2 {
            common: self.common.clone(),
            signing_common: self.signing_common.clone(),
            nonces: self.nonces,
            commitments1: messages,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

// ************

#[derive(Clone)]
struct VerifyCommitmentsBroadcast2 {
    common: SigningCeremonyCommon,
    signing_common: SigningStateCommonInfo,
    nonces: SecretNoncePair,
    commitments1: HashMap<usize, Comm1>,
}

derive_display!(VerifyCommitmentsBroadcast2);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for VerifyCommitmentsBroadcast2 {
    type Message = VerifyComm2;

    fn init(&self) -> Self::Message {
        // TODO: use map instead
        let mut data = Vec::with_capacity(self.common.all_idxs.len());

        for idx in &self.common.all_idxs {
            // TODO: is there a way to avoid unwrapping here?
            data.push(self.commitments1.get(&idx).cloned().unwrap());
        }

        VerifyComm2 { data }
    }

    should_delay!(SigningData::LocalSigStage3);

    fn process(self, messages: HashMap<usize, Self::Message>) -> SigningStageResult {
        let verified_commitments = match verify_broadcasts(&self.common.all_idxs, &messages) {
            Ok(comms) => comms,
            Err(blamed_parties) => {
                return StageResult::Error(blamed_parties);
            }
        };

        slog::debug!(
            self.common.logger,
            "Initial commitments have been correctly broadcast for ceremony: [todo]"
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

#[derive(Clone)]
struct LocalSigStage3 {
    signing_common: SigningStateCommonInfo,
    common: SigningCeremonyCommon,
    nonces: SecretNoncePair,
    commitments: Vec<Comm1>,
}

derive_display!(LocalSigStage3);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for LocalSigStage3 {
    type Message = LocalSig3;

    fn init(&self) -> Self::Message {
        slog::trace!(
            self.common.logger,
            "Generating local sig for ceremony [todo]"
        );

        frost::generate_local_sig(
            &self.signing_common.data.0,
            &self.signing_common.key.key_share,
            &self.nonces,
            &self.commitments,
            self.common.own_idx,
            &self.common.all_idxs,
        )
    }

    should_delay!(SigningData::VerifyLocalSigsStage4);

    fn process(self, messages: HashMap<usize, Self::Message>) -> SigningStageResult {
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

#[derive(Clone)]
struct VerifyLocalSigsBroadcastStage4 {
    common: SigningCeremonyCommon,
    signing_common: SigningStateCommonInfo,
    commitments: Vec<Comm1>,
    local_sigs: HashMap<usize, LocalSig3>,
}

derive_display!(VerifyLocalSigsBroadcastStage4);

impl BroadcastStageProcessor<SigningData, SchnorrSignature> for VerifyLocalSigsBroadcastStage4 {
    type Message = VerifyLocalSig4;

    fn init(&self) -> Self::Message {
        let mut data = Vec::with_capacity(self.common.all_idxs.len());

        for idx in &self.common.all_idxs {
            data.push(self.local_sigs.get(&idx).cloned().unwrap());
        }

        VerifyLocalSig4 { data }.into()
    }

    fn should_delay(&self, _: &SigningData) -> bool {
        false
    }

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
            Ok(sig) => StageResult::Done(sig.into()),
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
    vcbs: &HashMap<usize, BroadcastVerificationMessage<T>>,
) -> Result<Vec<T>, Vec<usize>> {
    let num_parties = signer_idxs.len();

    // Sanity check: we should have N messages, each containing N messages
    assert_eq!(vcbs.len(), num_parties);
    for (_, m) in vcbs {
        assert_eq!(m.data.len(), num_parties);
    }

    let threshold = threshold_from_share_count(num_parties);

    // NOTE: ideally we wouldn't need to serialize the messages again here, but
    // we can't use T as key directly (in our case it holds third-party structs)
    // and delaying deserialization when we receive these over p2p would would make
    // our code more complicated than necessary.

    let mut agreed_on_values: Vec<T> = Vec::with_capacity(num_parties);

    let mut blamed_parties = vec![];

    'outer: for i in 0..num_parties {
        let mut value_counts = HashMap::<Vec<u8>, usize>::new();
        for (_, m) in vcbs {
            let data = bincode::serialize(&m.data[i]).unwrap();
            *value_counts.entry(data).or_default() += 1;
        }

        for (data, count) in value_counts {
            if count > threshold {
                let data = bincode::deserialize(&data).unwrap();
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
    let mut vcbs = HashMap::new();

    // There is a concensus on each of the values,
    // even though some parties disagree on some values

    let all_messages = vec![
        vec![1, 1, 1, 1], // "correct" message
        vec![1, 2, 1, 1],
        vec![2, 1, 2, 1],
        vec![1, 1, 1, 2],
    ];

    for (i, m) in all_messages.into_iter().enumerate() {
        vcbs.insert(i + 1, BroadcastVerificationMessage { data: m });
    }

    assert_eq!(
        verify_broadcasts(&[1, 2, 3, 4], &vcbs),
        Ok(vec![1, 1, 1, 1])
    );
}

#[test]
fn check_incorrect_broadcast() {
    let mut vcbs = HashMap::new();

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
        vcbs.insert(i + 1, BroadcastVerificationMessage { data: m });
    }

    assert_eq!(verify_broadcasts(&[1, 2, 3, 4], &vcbs), Err(vec![2, 4]));
}
