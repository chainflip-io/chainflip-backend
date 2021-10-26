use std::collections::HashMap;

use pallet_cf_vaults::CeremonyId;

use crate::multisig::client;

use client::{
    common::{
        broadcast::{verify_broadcasts, BroadcastStage, BroadcastStageProcessor, DataToSend},
        CeremonyCommon, KeygenResult, StageResult,
    },
    keygen, ThresholdParameters,
};

use crate::multisig::crypto::KeyShare;

use keygen::{
    keygen_data::{Comm1, Complaints4, KeygenData, SecretShare3, VerifyComm2, VerifyComplaints5},
    keygen_frost::{
        derive_aggregate_pubkey, derive_local_pubkeys_for_parties, generate_keygen_context,
        generate_shares_and_commitment, validate_commitments, verify_share, DKGCommitment,
        DKGUnverifiedCommitment, ShamirShare,
    },
    KeygenP2PSender,
};

type KeygenCeremonyCommon = CeremonyCommon<KeygenData, KeygenP2PSender>;

/// Stage 1: Sample a secret, generate sharing polynomial coefficients for it
/// and a ZKP of the secret. Broadcast commitments to the coefficients and the ZKP.
#[derive(Clone)]
pub struct AwaitCommitments1 {
    common: KeygenCeremonyCommon,
    own_commitment: DKGUnverifiedCommitment,
    shares: HashMap<usize, ShamirShare>,
}

impl AwaitCommitments1 {
    pub fn new(ceremony_id: CeremonyId, common: KeygenCeremonyCommon) -> Self {
        let params = ThresholdParameters::from_share_count(common.all_idxs.len());

        let context = generate_keygen_context(ceremony_id);

        let (shares, own_commitment) =
            generate_shares_and_commitment(&context, common.own_idx, params);

        AwaitCommitments1 {
            common,
            own_commitment,
            shares,
        }
    }
}

derive_display_as_type_name!(AwaitCommitments1);

impl BroadcastStageProcessor<KeygenData, KeygenResult> for AwaitCommitments1 {
    type Message = Comm1;

    fn init(&self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(self.own_commitment.clone())
    }

    fn should_delay(&self, m: &KeygenData) -> bool {
        matches!(m, KeygenData::Verify2(_))
    }

    fn process(
        self,
        messages: HashMap<usize, Self::Message>,
    ) -> StageResult<KeygenData, KeygenResult> {
        // We have received commitments from everyone, for now just need to
        // go through another round to verify consistent broadcasts

        let processor = VerifyCommitmentsBroadcast2 {
            common: self.common.clone(),
            commitments: messages,
            shares_to_send: self.shares,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

/// Stage 2: verify broadcasts of Stage 1 data
#[derive(Clone)]
struct VerifyCommitmentsBroadcast2 {
    common: KeygenCeremonyCommon,
    commitments: HashMap<usize, Comm1>,
    shares_to_send: HashMap<usize, ShamirShare>,
}

derive_display_as_type_name!(VerifyCommitmentsBroadcast2);

impl BroadcastStageProcessor<KeygenData, KeygenResult> for VerifyCommitmentsBroadcast2 {
    type Message = VerifyComm2;

    fn init(&self) -> DataToSend<Self::Message> {
        let data = self
            .common
            .all_idxs
            .iter()
            .map(|idx| {
                self.commitments
                    .get(&idx)
                    .cloned()
                    .expect("all indexes should be present at this point")
            })
            .collect();

        DataToSend::Broadcast(VerifyComm2 { data })
    }

    fn should_delay(&self, m: &KeygenData) -> bool {
        matches!(m, KeygenData::SecretShares3(_))
    }

    fn process(
        self,
        messages: std::collections::HashMap<usize, Self::Message>,
    ) -> StageResult<KeygenData, KeygenResult> {
        let commitments = match verify_broadcasts(&self.common.all_idxs, &messages) {
            Ok(comms) => comms,
            Err(blamed_parties) => return StageResult::Error(blamed_parties),
        };

        let context = generate_keygen_context(self.common.ceremony_id);

        let commitments = match validate_commitments(commitments, &context) {
            Ok(comms) => comms,
            Err(blamed_parties) => return StageResult::Error(blamed_parties),
        };

        slog::debug!(
            self.common.logger,
            "Initial commitments have been correctly broadcast"
        );

        let processor = SecretSharesStage3 {
            common: self.common.clone(),
            commitments,
            shares: self.shares_to_send,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

/// Stage 3: distribute (distinct) secret shares of our secret to each party
#[derive(Clone)]
struct SecretSharesStage3 {
    common: KeygenCeremonyCommon,
    // commitments (verified to have been broadcast correctly)
    commitments: Vec<DKGCommitment>,
    shares: HashMap<usize, ShamirShare>,
}

derive_display_as_type_name!(SecretSharesStage3);

impl BroadcastStageProcessor<KeygenData, KeygenResult> for SecretSharesStage3 {
    type Message = SecretShare3;

    fn init(&self) -> DataToSend<Self::Message> {
        // With everyone commited to their secrets and sharing polynomial coefficients
        // we can now send the *distinct* secret shares to each party

        DataToSend::Private(self.shares.clone())
    }

    fn should_delay(&self, m: &KeygenData) -> bool {
        matches!(m, KeygenData::Complaints4(_))
    }

    fn process(
        self,
        shares: HashMap<usize, Self::Message>,
    ) -> StageResult<KeygenData, KeygenResult> {
        // IMPORTANT! As the messages for this stage are sent in secret, it is possible
        // for a malicious party to send us invalid data without us being able to prove
        // that. Because of that, we can't simply terminate our protocol here.
        // TODO: implement the "blaming" stage

        let bad_parties: Vec<_> = shares
            .iter()
            .filter_map(|(sender_idx, share)| {
                if verify_share(share, &self.commitments[sender_idx - 1]) {
                    None
                } else {
                    Some(*sender_idx)
                }
            })
            .collect();

        debug_assert!(bad_parties.is_empty());

        let processor = ComplaintsStage4 {
            common: self.common.clone(),
            commitments: self.commitments,
            shares,
            complaints: bad_parties,
        };
        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

/// During this stage parties have a chance to complain about
/// a party sending a secret share that isn't valid when checked
/// against the commitments
#[derive(Clone)]
struct ComplaintsStage4 {
    common: KeygenCeremonyCommon,
    // commitments (verified to have been broadcast correctly)
    commitments: Vec<DKGCommitment>,
    shares: HashMap<usize, ShamirShare>,
    complaints: Vec<usize>,
}

derive_display_as_type_name!(ComplaintsStage4);

impl BroadcastStageProcessor<KeygenData, KeygenResult> for ComplaintsStage4 {
    type Message = Complaints4;

    fn init(&self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(Complaints4(self.complaints.clone()))
    }

    fn should_delay(&self, m: &KeygenData) -> bool {
        matches!(m, KeygenData::VerifyComplaints5(_))
    }

    fn process(
        self,
        messages: HashMap<usize, Self::Message>,
    ) -> StageResult<KeygenData, KeygenResult> {
        let processor = VerfiyComplaintsBroadcastStage5 {
            common: self.common.clone(),
            received_complaints: messages,
            commitments: self.commitments,
            shares: self.shares,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

#[derive(Clone)]
struct VerfiyComplaintsBroadcastStage5 {
    common: KeygenCeremonyCommon,
    received_complaints: HashMap<usize, Complaints4>,
    commitments: Vec<DKGCommitment>,
    shares: HashMap<usize, ShamirShare>,
}

derive_display_as_type_name!(VerfiyComplaintsBroadcastStage5);

impl BroadcastStageProcessor<KeygenData, KeygenResult> for VerfiyComplaintsBroadcastStage5 {
    type Message = VerifyComplaints5;

    fn init(&self) -> DataToSend<Self::Message> {
        let data = self
            .common
            .all_idxs
            .iter()
            .map(|idx| {
                self.received_complaints
                    .get(&idx)
                    .cloned()
                    .expect("all indexes should be present at this point")
            })
            .collect();

        DataToSend::Broadcast(VerifyComplaints5 { data })
    }

    fn should_delay(&self, _: &KeygenData) -> bool {
        // TODO: delay blaming stage messages once implemented
        false
    }

    fn process(
        self,
        messages: HashMap<usize, Self::Message>,
    ) -> StageResult<KeygenData, KeygenResult> {
        let verified_complaints = match verify_broadcasts(&self.common.all_idxs, &messages) {
            Ok(comms) => comms,
            Err(blamed_parties) => {
                return StageResult::Error(blamed_parties);
            }
        };

        if verified_complaints.iter().any(|c| !c.0.is_empty()) {
            todo!("Implement blaming stage");
        }

        // if all complaints are empty, we can finalize the ceremony
        let keygen_result = {
            let secret_share = self
                .shares
                .values()
                .into_iter()
                .map(|share| share.value)
                .reduce(|acc, share| acc + share)
                .expect("shares should be non-empty");

            // TODO: delete all received shares

            let agg_pubkey = derive_aggregate_pubkey(&self.commitments);

            let params = ThresholdParameters::from_share_count(self.common.all_idxs.len());

            let party_public_keys = derive_local_pubkeys_for_parties(params, &self.commitments);

            // TODO: can we be certain that the sharing polynomial
            // is the right size (according to `t`)?

            KeygenResult {
                key_share: KeyShare {
                    y: agg_pubkey,
                    x_i: secret_share,
                },
                party_public_keys,
            }
        };

        StageResult::Done(keygen_result)
    }
}
