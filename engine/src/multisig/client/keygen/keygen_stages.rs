use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::multisig::client::common::{
    BroadcastStageName, CeremonyFailureReason, KeygenFailureReason,
};
use crate::multisig::client::{self, KeygenResult, KeygenResultInfo};
use crate::{common::format_iterator, logging::KEYGEN_REJECTED_INCOMPATIBLE};

use cf_traits::AuthorityCount;
use client::{
    common::{
        broadcast::{verify_broadcasts, BroadcastStage, BroadcastStageProcessor, DataToSend},
        CeremonyCommon, StageResult,
    },
    keygen, ThresholdParameters,
};
use itertools::Itertools;
use sp_core::H256;

use crate::multisig::crypto::{ECPoint, KeyShare};

use keygen::{
    keygen_data::{
        BlameResponse6, Comm1, Complaints4, KeygenData, SecretShare3, VerifyComm2,
        VerifyComplaints5,
    },
    keygen_frost::{
        derive_aggregate_pubkey, generate_shares_and_commitment, validate_commitments,
        verify_share, DKGCommitment, DKGUnverifiedCommitment, IncomingShares, OutgoingShares,
    },
};

use super::keygen_data::{HashComm1, VerifyHashComm2};
use super::keygen_frost::{
    compute_secret_key_share, derive_local_pubkeys_for_parties, generate_hash_commitment,
    ShamirShare, ValidAggregateKey,
};
use super::{keygen_data::VerifyBlameResponses7, HashContext};

type KeygenStageResult<P> = StageResult<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>;

pub struct HashCommitments1<P: ECPoint> {
    common: CeremonyCommon,
    allow_high_pubkey: bool,
    own_commitment: DKGUnverifiedCommitment<P>,
    hash_commitment: H256,
    shares: OutgoingShares<P>,
    context: HashContext,
}

derive_display_as_type_name!(HashCommitments1<P: ECPoint>);

impl<P: ECPoint> HashCommitments1<P> {
    pub fn new(mut common: CeremonyCommon, allow_high_pubkey: bool, context: HashContext) -> Self {
        // Generate the secret polynomial and commit to it by hashing all public coefficients
        let params = ThresholdParameters::from_share_count(
            common.all_idxs.len().try_into().expect("too many parties"),
        );

        let (shares, own_commitment) =
            generate_shares_and_commitment(&mut common.rng, &context, common.own_idx, params);

        let hash_commitment = generate_hash_commitment(&own_commitment);

        HashCommitments1 {
            common,
            allow_high_pubkey,
            own_commitment,
            hash_commitment,
            shares,
            context,
        }
    }
}

impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for HashCommitments1<P>
{
    type Message = HashComm1;

    fn init(&mut self) -> DataToSend<Self::Message> {
        // We don't want to reveal the public coefficients yet, so sending the hash commitment only
        DataToSend::Broadcast(HashComm1(self.hash_commitment))
    }

    fn should_delay(&self, m: &KeygenData<P>) -> bool {
        matches!(m, KeygenData::VerifyHashComm2(_))
    }

    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> StageResult<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason> {
        // Prepare for broadcast verification
        let processor = VerifyHashCommitmentsBroadcast2 {
            common: self.common.clone(),
            own_commitment: self.own_commitment,
            hash_commitments: messages,
            shares_to_send: self.shares,
            allow_high_pubkey: self.allow_high_pubkey,
            context: self.context,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

pub struct VerifyHashCommitmentsBroadcast2<P: ECPoint> {
    common: CeremonyCommon,
    allow_high_pubkey: bool,
    own_commitment: DKGUnverifiedCommitment<P>,
    hash_commitments: BTreeMap<AuthorityCount, Option<HashComm1>>,
    shares_to_send: OutgoingShares<P>,
    context: HashContext,
}

derive_display_as_type_name!(VerifyHashCommitmentsBroadcast2<P: ECPoint>);

impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for VerifyHashCommitmentsBroadcast2<P>
{
    type Message = VerifyHashComm2;

    fn init(&mut self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(VerifyHashComm2 {
            data: self.hash_commitments.clone(),
        })
    }

    fn should_delay(&self, m: &KeygenData<P>) -> bool {
        matches!(m, KeygenData::Comm1(_))
    }

    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> StageResult<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason> {
        let hash_commitments = match verify_broadcasts(messages, &self.common.logger) {
            Ok(hash_commitments) => hash_commitments,
            Err((reported_parties, abort_reason)) => {
                return KeygenStageResult::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(
                        abort_reason,
                        BroadcastStageName::HashCommitments,
                    ),
                );
            }
        };

        slog::debug!(
            self.common.logger,
            "{} have been correctly broadcast",
            BroadcastStageName::HashCommitments
        );

        // Just saving hash commitments for now. We will use them
        // once the parties reveal their public coefficients (next two stages)

        let processor = AwaitCommitments1 {
            common: self.common.clone(),
            hash_commitments,
            own_commitment: self.own_commitment,
            shares: self.shares_to_send,
            allow_high_pubkey: self.allow_high_pubkey,
            context: self.context,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

/// Stage 1: Sample a secret, generate sharing polynomial coefficients for it
/// and a ZKP of the secret. Broadcast commitments to the coefficients and the ZKP.
pub struct AwaitCommitments1<P: ECPoint> {
    common: CeremonyCommon,
    hash_commitments: BTreeMap<AuthorityCount, HashComm1>,
    own_commitment: DKGUnverifiedCommitment<P>,
    /// Shares generated by us for other parties (secret)
    shares: OutgoingShares<P>,
    allow_high_pubkey: bool,
    /// Context to prevent replay attacks
    context: HashContext,
}

derive_display_as_type_name!(AwaitCommitments1<P: ECPoint>);

impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for AwaitCommitments1<P>
{
    type Message = Comm1<P>;

    fn init(&mut self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(self.own_commitment.clone())
    }

    fn should_delay(&self, m: &KeygenData<P>) -> bool {
        matches!(m, KeygenData::Verify2(_))
    }

    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        // We have received commitments from everyone, for now just need to
        // go through another round to verify consistent broadcasts

        let processor = VerifyCommitmentsBroadcast2 {
            common: self.common.clone(),
            hash_commitments: self.hash_commitments,
            commitments: messages,
            shares_to_send: self.shares,
            allow_high_pubkey: self.allow_high_pubkey,
            context: self.context,
        };

        let stage = BroadcastStage::<_, _, _, P, _>::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

/// Stage 2: verify broadcasts of Stage 1 data
struct VerifyCommitmentsBroadcast2<P: ECPoint> {
    common: CeremonyCommon,
    hash_commitments: BTreeMap<AuthorityCount, HashComm1>,
    commitments: BTreeMap<AuthorityCount, Option<Comm1<P>>>,
    shares_to_send: OutgoingShares<P>,
    allow_high_pubkey: bool,
    context: HashContext,
}

derive_display_as_type_name!(VerifyCommitmentsBroadcast2<P: ECPoint>);

impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for VerifyCommitmentsBroadcast2<P>
{
    type Message = VerifyComm2<P>;

    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.commitments.clone();

        DataToSend::Broadcast(VerifyComm2 { data })
    }

    fn should_delay(&self, m: &KeygenData<P>) -> bool {
        matches!(m, KeygenData::SecretShares3(_))
    }

    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        let commitments = match verify_broadcasts(messages, &self.common.logger) {
            Ok(comms) => comms,
            Err((reported_parties, abort_reason)) => {
                return KeygenStageResult::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(
                        abort_reason,
                        BroadcastStageName::InitialCommitments,
                    ),
                );
            }
        };

        let commitments = match validate_commitments(
            commitments,
            self.hash_commitments,
            &self.context,
            &self.common.logger,
        ) {
            Ok(comms) => comms,
            Err((blamed_parties, reason)) => {
                return StageResult::Error(blamed_parties, CeremonyFailureReason::Other(reason));
            }
        };

        slog::debug!(
            self.common.logger,
            "{} have been correctly broadcast",
            BroadcastStageName::InitialCommitments
        );

        // At this point we know everyone's commitments, which can already be
        // used to derive the resulting aggregate public key. Before proceeding
        // with the ceremony, we need to make sure that the key is compatible
        // with the Key Manager contract, aborting if it isn't.

        match derive_aggregate_pubkey(&commitments, self.allow_high_pubkey) {
            Ok(agg_pubkey) => {
                let processor = SecretSharesStage3 {
                    common: self.common.clone(),
                    commitments,
                    shares: self.shares_to_send,
                    agg_pubkey,
                };

                let stage = BroadcastStage::new(processor, self.common);

                StageResult::NextStage(Box::new(stage))
            }
            Err(err) => {
                slog::debug!(
                    self.common.logger,
                    #KEYGEN_REJECTED_INCOMPATIBLE,
                    "Aborting keygen ceremony: {}", err
                );
                // It is nobody's fault that the key is not compatible,
                // so we abort with an empty list of responsible nodes
                // to let the State Chain restart the ceremony
                StageResult::Error(
                    BTreeSet::new(),
                    CeremonyFailureReason::Other(KeygenFailureReason::KeyNotCompatible),
                )
            }
        }
    }
}

/// Stage 3: distribute (distinct) secret shares of our secret to each party
struct SecretSharesStage3<P: ECPoint> {
    common: CeremonyCommon,
    // commitments (verified to have been broadcast correctly)
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
    shares: OutgoingShares<P>,
    agg_pubkey: ValidAggregateKey<P>,
}

derive_display_as_type_name!(SecretSharesStage3<P: ECPoint>);

impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for SecretSharesStage3<P>
{
    type Message = SecretShare3<P>;

    fn init(&mut self) -> DataToSend<Self::Message> {
        // With everyone committed to their secrets and sharing polynomial coefficients
        // we can now send the *distinct* secret shares to each party

        DataToSend::Private(self.shares.0.clone())
    }

    fn should_delay(&self, m: &KeygenData<P>) -> bool {
        matches!(m, KeygenData::Complaints4(_))
    }

    fn process(
        self,
        incoming_shares: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        // As the messages for this stage are sent in secret, it is possible
        // for a malicious party to send us invalid data (or not send anything
        // at all) without us being able to prove that. Because of that, we
        // can't simply terminate our protocol here.

        let mut bad_parties = BTreeSet::new();
        let verified_shares: BTreeMap<AuthorityCount, Self::Message> = incoming_shares
            .into_iter()
            .filter_map(|(sender_idx, share_opt)| {
                if let Some(share) = share_opt {
                    if verify_share(&share, &self.commitments[&sender_idx], self.common.own_idx) {
                        Some((sender_idx, share))
                    } else {
                        slog::warn!(
                            self.common.logger,
                            "Received invalid secret share from party: {}",
                            sender_idx
                        );

                        bad_parties.insert(sender_idx);
                        None
                    }
                } else {
                    slog::warn!(
                        self.common.logger,
                        "Received no secret share from party: {}",
                        sender_idx
                    );

                    bad_parties.insert(sender_idx);
                    None
                }
            })
            .collect();

        let processor = ComplaintsStage4 {
            common: self.common.clone(),
            commitments: self.commitments,
            agg_pubkey: self.agg_pubkey,
            shares: IncomingShares(verified_shares),
            outgoing_shares: self.shares,
            complaints: bad_parties,
        };
        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

/// During this stage parties have a chance to complain about
/// a party sending a secret share that isn't valid when checked
/// against the commitments
struct ComplaintsStage4<P: ECPoint> {
    common: CeremonyCommon,
    // commitments (verified to have been broadcast correctly)
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
    agg_pubkey: ValidAggregateKey<P>,
    /// Shares sent to us from other parties (secret)
    shares: IncomingShares<P>,
    outgoing_shares: OutgoingShares<P>,
    complaints: BTreeSet<AuthorityCount>,
}

derive_display_as_type_name!(ComplaintsStage4<P: ECPoint>);

impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for ComplaintsStage4<P>
{
    type Message = Complaints4;

    fn init(&mut self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(Complaints4(self.complaints.clone()))
    }

    fn should_delay(&self, m: &KeygenData<P>) -> bool {
        matches!(m, KeygenData::VerifyComplaints5(_))
    }

    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        let processor = VerifyComplaintsBroadcastStage5 {
            common: self.common.clone(),
            agg_pubkey: self.agg_pubkey,
            received_complaints: messages,
            commitments: self.commitments,
            shares: self.shares,
            outgoing_shares: self.outgoing_shares,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

struct VerifyComplaintsBroadcastStage5<P: ECPoint> {
    common: CeremonyCommon,
    agg_pubkey: ValidAggregateKey<P>,
    received_complaints: BTreeMap<AuthorityCount, Option<Complaints4>>,
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
    shares: IncomingShares<P>,
    outgoing_shares: OutgoingShares<P>,
}

derive_display_as_type_name!(VerifyComplaintsBroadcastStage5<P: ECPoint>);

impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for VerifyComplaintsBroadcastStage5<P>
{
    type Message = VerifyComplaints5;

    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.received_complaints.clone();

        DataToSend::Broadcast(VerifyComplaints5 { data })
    }

    fn should_delay(&self, data: &KeygenData<P>) -> bool {
        matches!(data, KeygenData::BlameResponse6(_))
    }

    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        let verified_complaints = match verify_broadcasts(messages, &self.common.logger) {
            Ok(comms) => comms,
            Err((reported_parties, abort_reason)) => {
                return KeygenStageResult::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(
                        abort_reason,
                        BroadcastStageName::Complaints,
                    ),
                );
            }
        };

        if verified_complaints.iter().all(|(_idx, c)| c.0.is_empty()) {
            // if all complaints are empty, we can finalize the ceremony
            return finalize_keygen(self.common, self.agg_pubkey, self.shares, &self.commitments);
        };

        // Some complaints have been issued, entering the blaming stage

        let idxs_to_report: BTreeSet<_> = verified_complaints
            .iter()
            .filter_map(|(idx_from, Complaints4(blamed_idxs))| {
                let has_invalid_idxs = !blamed_idxs.iter().all(|idx_blamed| {
                    if self.common.is_idx_valid(*idx_blamed) {
                        true
                    } else {
                        slog::warn!(
                            self.common.logger,
                            "Invalid index in complaint: {}",
                            format_iterator(blamed_idxs)
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
            let processor = BlameResponsesStage6 {
                common: self.common.clone(),
                complaints: verified_complaints,
                agg_pubkey: self.agg_pubkey,
                shares: self.shares,
                outgoing_shares: self.outgoing_shares,
                commitments: self.commitments,
            };

            let stage = BroadcastStage::new(processor, self.common);

            StageResult::NextStage(Box::new(stage))
        } else {
            StageResult::Error(
                idxs_to_report,
                CeremonyFailureReason::Other(KeygenFailureReason::InvalidComplaint),
            )
        }
    }
}

fn finalize_keygen<KeygenData, P: ECPoint>(
    common: CeremonyCommon,
    agg_pubkey: ValidAggregateKey<P>,
    secret_shares: IncomingShares<P>,
    commitments: &BTreeMap<AuthorityCount, DKGCommitment<P>>,
) -> StageResult<KeygenData, KeygenResultInfo<P>, KeygenFailureReason> {
    let params = ThresholdParameters::from_share_count(common.all_idxs.len() as AuthorityCount);

    let keygen_result_info = KeygenResultInfo {
        key: Arc::new(KeygenResult {
            key_share: KeyShare {
                y: agg_pubkey.0,
                x_i: compute_secret_key_share(secret_shares),
            },
            party_public_keys: derive_local_pubkeys_for_parties(params, commitments),
        }),
        validator_map: common.validator_mapping,
        params,
    };

    StageResult::Done(keygen_result_info)
}

struct BlameResponsesStage6<P: ECPoint> {
    common: CeremonyCommon,
    complaints: BTreeMap<AuthorityCount, Complaints4>,
    agg_pubkey: ValidAggregateKey<P>,
    shares: IncomingShares<P>,
    outgoing_shares: OutgoingShares<P>,
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
}

derive_display_as_type_name!(BlameResponsesStage6<P: ECPoint>);

impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for BlameResponsesStage6<P>
{
    type Message = BlameResponse6<P>;

    fn init(&mut self) -> DataToSend<Self::Message> {
        // Indexes at which to reveal/broadcast secret shares
        let idxs_to_reveal: Vec<_> = self
            .complaints
            .iter()
            .filter_map(|(idx, complaint)| {
                if complaint.0.contains(&self.common.own_idx) {
                    slog::warn!(
                        self.common.logger,
                        "[{}] we are blamed by [{}]",
                        self.common.own_idx,
                        idx
                    );

                    Some(*idx)
                } else {
                    None
                }
            })
            .collect();

        // TODO: put a limit on how many shares to reveal?
        let data = DataToSend::Broadcast(BlameResponse6(
            idxs_to_reveal
                .iter()
                .map(|idx| {
                    slog::debug!(self.common.logger, "revealing share for [{}]", *idx);
                    (*idx, self.outgoing_shares.0[idx].clone())
                })
                .collect(),
        ));

        // Outgoing shares are no longer needed, so we zeroize them
        drop(std::mem::take(&mut self.outgoing_shares));

        data
    }

    fn should_delay(&self, data: &KeygenData<P>) -> bool {
        matches!(data, KeygenData::VerifyBlameResponses7(_))
    }

    fn process(
        self,
        blame_responses: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        let processor = VerifyBlameResponsesBroadcastStage7 {
            common: self.common.clone(),
            complaints: self.complaints,
            agg_pubkey: self.agg_pubkey,
            blame_responses,
            shares: self.shares,
            commitments: self.commitments,
        };

        let stage = BroadcastStage::new(processor, self.common);

        StageResult::NextStage(Box::new(stage))
    }
}

struct VerifyBlameResponsesBroadcastStage7<P: ECPoint> {
    common: CeremonyCommon,
    complaints: BTreeMap<AuthorityCount, Complaints4>,
    agg_pubkey: ValidAggregateKey<P>,
    // Blame responses received from other parties in the previous communication round
    blame_responses: BTreeMap<AuthorityCount, Option<BlameResponse6<P>>>,
    shares: IncomingShares<P>,
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
}

derive_display_as_type_name!(VerifyBlameResponsesBroadcastStage7<P: ECPoint>);

/// Checks for sender_idx that their blame response contains exactly
/// a share for each party that blamed them
fn is_blame_response_complete<P: ECPoint>(
    sender_idx: AuthorityCount,
    response: &BlameResponse6<P>,
    complaints: &BTreeMap<AuthorityCount, Complaints4>,
) -> bool {
    let expected_idxs_iter = complaints
        .iter()
        .filter_map(|(blamer_idx, Complaints4(blamed_idxs))| {
            if blamed_idxs.contains(&sender_idx) {
                Some(blamer_idx)
            } else {
                None
            }
        })
        .sorted();

    // Note: the keys in BTreeMap are already sorted
    Iterator::eq(response.0.keys(), expected_idxs_iter)
}

impl<P: ECPoint> VerifyBlameResponsesBroadcastStage7<P> {
    /// Check that blame responses contain all (and only) the requested shares, and that all the shares are valid.
    /// If all responses are valid, returns shares destined for us along with the corresponding index. Otherwise,
    /// returns a list of party indexes who provided invalid responses.
    fn check_blame_responses(
        &self,
        blame_responses: BTreeMap<AuthorityCount, BlameResponse6<P>>,
    ) -> Result<BTreeMap<AuthorityCount, ShamirShare<P>>, BTreeSet<AuthorityCount>> {
        let (shares_for_us, bad_parties): (Vec<_>, BTreeSet<_>) = blame_responses
            .iter()
            .map(|(sender_idx, response)| {
                if !is_blame_response_complete(*sender_idx, response, &self.complaints) {
                    slog::warn!(
                        self.common.logger,
                        "Incomplete blame response from party: {}",
                        sender_idx
                    );

                    return Err(sender_idx);
                }

                if !response.0.iter().all(|(dest_idx, share)| {
                    verify_share(share, &self.commitments[sender_idx], *dest_idx)
                }) {
                    slog::warn!(
                        self.common.logger,
                        "Invalid secret share in a blame response from party: {}",
                        sender_idx
                    );

                    return Err(sender_idx);
                }

                Ok((*sender_idx, response.0.get(&self.common.own_idx)))
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

impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for VerifyBlameResponsesBroadcastStage7<P>
{
    type Message = VerifyBlameResponses7<P>;

    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.blame_responses.clone();

        DataToSend::Broadcast(VerifyBlameResponses7 { data })
    }

    fn should_delay(&self, _: &KeygenData<P>) -> bool {
        false
    }

    fn process(
        mut self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        slog::debug!(
            self.common.logger,
            "Processing verifications for {}",
            BroadcastStageName::BlameResponses
        );

        let verified_responses = match verify_broadcasts(messages, &self.common.logger) {
            Ok(comms) => comms,
            Err((reported_parties, abort_reason)) => {
                return KeygenStageResult::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(
                        abort_reason,
                        BroadcastStageName::BlameResponses,
                    ),
                );
            }
        };

        match self.check_blame_responses(verified_responses) {
            Ok(shares_for_us) => {
                for (sender_idx, share) in shares_for_us {
                    self.shares.0.insert(sender_idx, share);
                }

                finalize_keygen(self.common, self.agg_pubkey, self.shares, &self.commitments)
            }
            Err(bad_parties) => StageResult::Error(
                bad_parties,
                CeremonyFailureReason::Other(KeygenFailureReason::InvalidBlameResponse),
            ),
        }
    }
}
