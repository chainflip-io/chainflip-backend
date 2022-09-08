use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::common::format_iterator;
use crate::multisig::client::common::{
    CeremonyFailureReason, CeremonyStageName, KeygenFailureReason,
};
use crate::multisig::client::{self, KeygenResult, KeygenResultInfo};

use async_trait::async_trait;
use cf_primitives::AuthorityCount;
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
        BlameResponse8, CoeffComm3, Complaints6, KeygenData, SecretShare5, VerifyCoeffComm4,
        VerifyComplaints7,
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
use super::{keygen_data::VerifyBlameResponses9, HashContext};

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

#[async_trait]
impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for HashCommitments1<P>
{
    type Message = HashComm1;
    const NAME: CeremonyStageName = CeremonyStageName::HashCommitments1;

    fn init(&mut self) -> DataToSend<Self::Message> {
        // We don't want to reveal the public coefficients yet, so sending the hash commitment only
        DataToSend::Broadcast(HashComm1(self.hash_commitment))
    }

    async fn process(
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

#[cfg(test)]
impl<P: ECPoint> VerifyHashCommitmentsBroadcast2<P> {
    pub fn new(
        common: CeremonyCommon,
        allow_high_pubkey: bool,
        own_commitment: DKGUnverifiedCommitment<P>,
        hash_commitments: BTreeMap<AuthorityCount, Option<HashComm1>>,
        shares_to_send: OutgoingShares<P>,
        context: HashContext,
    ) -> Self {
        Self {
            common,
            allow_high_pubkey,
            own_commitment,
            hash_commitments,
            shares_to_send,
            context,
        }
    }
}

derive_display_as_type_name!(VerifyHashCommitmentsBroadcast2<P: ECPoint>);

#[async_trait]
impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for VerifyHashCommitmentsBroadcast2<P>
{
    type Message = VerifyHashComm2;
    const NAME: CeremonyStageName = CeremonyStageName::VerifyHashCommitmentsBroadcast2;

    fn init(&mut self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(VerifyHashComm2 {
            data: self.hash_commitments.clone(),
        })
    }

    async fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> StageResult<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason> {
        let hash_commitments = match verify_broadcasts(messages, &self.common.logger) {
            Ok(hash_commitments) => hash_commitments,
            Err((reported_parties, abort_reason)) => {
                return KeygenStageResult::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(abort_reason, Self::NAME),
                );
            }
        };

        slog::debug!(self.common.logger, "{} is successful", Self::NAME);

        // Just saving hash commitments for now. We will use them
        // once the parties reveal their public coefficients (next two stages)

        let processor = CoefficientCommitments3 {
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

/// Stage 3: Sample a secret, generate sharing polynomial coefficients for it
/// and a ZKP of the secret. Broadcast commitments to the coefficients and the ZKP.
pub struct CoefficientCommitments3<P: ECPoint> {
    common: CeremonyCommon,
    hash_commitments: BTreeMap<AuthorityCount, HashComm1>,
    own_commitment: DKGUnverifiedCommitment<P>,
    /// Shares generated by us for other parties (secret)
    shares: OutgoingShares<P>,
    allow_high_pubkey: bool,
    /// Context to prevent replay attacks
    context: HashContext,
}

derive_display_as_type_name!(CoefficientCommitments3<P: ECPoint>);

#[async_trait]
impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for CoefficientCommitments3<P>
{
    type Message = CoeffComm3<P>;
    const NAME: CeremonyStageName = CeremonyStageName::CoefficientCommitments3;

    fn init(&mut self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(self.own_commitment.clone())
    }

    async fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        // We have received commitments from everyone, for now just need to
        // go through another round to verify consistent broadcasts

        let processor = VerifyCommitmentsBroadcast4 {
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

/// Stage 4: verify broadcasts of Stage 3 data
struct VerifyCommitmentsBroadcast4<P: ECPoint> {
    common: CeremonyCommon,
    hash_commitments: BTreeMap<AuthorityCount, HashComm1>,
    commitments: BTreeMap<AuthorityCount, Option<CoeffComm3<P>>>,
    shares_to_send: OutgoingShares<P>,
    allow_high_pubkey: bool,
    context: HashContext,
}

derive_display_as_type_name!(VerifyCommitmentsBroadcast4<P: ECPoint>);

#[async_trait]
impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for VerifyCommitmentsBroadcast4<P>
{
    type Message = VerifyCoeffComm4<P>;
    const NAME: CeremonyStageName = CeremonyStageName::VerifyCommitmentsBroadcast4;

    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.commitments.clone();

        DataToSend::Broadcast(VerifyCoeffComm4 { data })
    }

    async fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        let commitments = match verify_broadcasts(messages, &self.common.logger) {
            Ok(comms) => comms,
            Err((reported_parties, abort_reason)) => {
                return KeygenStageResult::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(abort_reason, Self::NAME),
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

        slog::debug!(self.common.logger, "{} is successful", Self::NAME);

        // At this point we know everyone's commitments, which can already be
        // used to derive the resulting aggregate public key. Before proceeding
        // with the ceremony, we need to make sure that the key is compatible
        // with the Key Manager contract, aborting if it isn't.

        match derive_aggregate_pubkey(&commitments, self.allow_high_pubkey) {
            Ok(agg_pubkey) => {
                let processor = SecretSharesStage5 {
                    common: self.common.clone(),
                    commitments,
                    shares: self.shares_to_send,
                    agg_pubkey,
                };

                let stage = BroadcastStage::new(processor, self.common);

                StageResult::NextStage(Box::new(stage))
            }
            Err(err) => {
                slog::debug!(self.common.logger, "Invalid pubkey: {}", err);
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

/// Stage 5: distribute (distinct) secret shares of our secret to each party
struct SecretSharesStage5<P: ECPoint> {
    common: CeremonyCommon,
    // commitments (verified to have been broadcast correctly)
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
    shares: OutgoingShares<P>,
    agg_pubkey: ValidAggregateKey<P>,
}

derive_display_as_type_name!(SecretSharesStage5<P: ECPoint>);

#[async_trait]
impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for SecretSharesStage5<P>
{
    type Message = SecretShare5<P>;
    const NAME: CeremonyStageName = CeremonyStageName::SecretSharesStage5;

    fn init(&mut self) -> DataToSend<Self::Message> {
        // With everyone committed to their secrets and sharing polynomial coefficients
        // we can now send the *distinct* secret shares to each party

        DataToSend::Private(self.shares.0.clone())
    }

    async fn process(
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

        let processor = ComplaintsStage6 {
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
struct ComplaintsStage6<P: ECPoint> {
    common: CeremonyCommon,
    // commitments (verified to have been broadcast correctly)
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
    agg_pubkey: ValidAggregateKey<P>,
    /// Shares sent to us from other parties (secret)
    shares: IncomingShares<P>,
    outgoing_shares: OutgoingShares<P>,
    complaints: BTreeSet<AuthorityCount>,
}

derive_display_as_type_name!(ComplaintsStage6<P: ECPoint>);

#[async_trait]
impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for ComplaintsStage6<P>
{
    type Message = Complaints6;
    const NAME: CeremonyStageName = CeremonyStageName::ComplaintsStage6;

    fn init(&mut self) -> DataToSend<Self::Message> {
        DataToSend::Broadcast(Complaints6(self.complaints.clone()))
    }

    async fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        let processor = VerifyComplaintsBroadcastStage7 {
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

struct VerifyComplaintsBroadcastStage7<P: ECPoint> {
    common: CeremonyCommon,
    agg_pubkey: ValidAggregateKey<P>,
    received_complaints: BTreeMap<AuthorityCount, Option<Complaints6>>,
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
    shares: IncomingShares<P>,
    outgoing_shares: OutgoingShares<P>,
}

derive_display_as_type_name!(VerifyComplaintsBroadcastStage7<P: ECPoint>);

#[async_trait]
impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for VerifyComplaintsBroadcastStage7<P>
{
    type Message = VerifyComplaints7;
    const NAME: CeremonyStageName = CeremonyStageName::VerifyComplaintsBroadcastStage7;

    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.received_complaints.clone();

        DataToSend::Broadcast(VerifyComplaints7 { data })
    }

    async fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        let verified_complaints = match verify_broadcasts(messages, &self.common.logger) {
            Ok(comms) => comms,
            Err((reported_parties, abort_reason)) => {
                return KeygenStageResult::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(abort_reason, Self::NAME),
                );
            }
        };

        if verified_complaints.iter().all(|(_idx, c)| c.0.is_empty()) {
            // if all complaints are empty, we can finalize the ceremony
            return finalize_keygen(self.common, self.agg_pubkey, self.shares, self.commitments)
                .await;
        };

        // Some complaints have been issued, entering the blaming stage

        let idxs_to_report: BTreeSet<_> = verified_complaints
            .iter()
            .filter_map(|(idx_from, Complaints6(blamed_idxs))| {
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
            let processor = BlameResponsesStage8 {
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

async fn finalize_keygen<KeygenData, P: ECPoint>(
    common: CeremonyCommon,
    agg_pubkey: ValidAggregateKey<P>,
    secret_shares: IncomingShares<P>,
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
) -> StageResult<KeygenData, KeygenResultInfo<P>, KeygenFailureReason> {
    let params = ThresholdParameters::from_share_count(common.all_idxs.len() as AuthorityCount);

    let party_public_keys =
        tokio::task::spawn_blocking(move || derive_local_pubkeys_for_parties(params, &commitments))
            .await
            .unwrap();

    let keygen_result_info = KeygenResultInfo {
        key: Arc::new(KeygenResult {
            key_share: KeyShare {
                y: agg_pubkey.0,
                x_i: compute_secret_key_share(secret_shares),
            },
            party_public_keys,
        }),
        validator_map: common.validator_mapping,
        params,
    };

    StageResult::Done(keygen_result_info)
}

struct BlameResponsesStage8<P: ECPoint> {
    common: CeremonyCommon,
    complaints: BTreeMap<AuthorityCount, Complaints6>,
    agg_pubkey: ValidAggregateKey<P>,
    shares: IncomingShares<P>,
    outgoing_shares: OutgoingShares<P>,
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
}

derive_display_as_type_name!(BlameResponsesStage8<P: ECPoint>);

#[async_trait]
impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for BlameResponsesStage8<P>
{
    type Message = BlameResponse8<P>;
    const NAME: CeremonyStageName = CeremonyStageName::BlameResponsesStage8;

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
        let data = DataToSend::Broadcast(BlameResponse8(
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

    async fn process(
        self,
        blame_responses: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        let processor = VerifyBlameResponsesBroadcastStage9 {
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

struct VerifyBlameResponsesBroadcastStage9<P: ECPoint> {
    common: CeremonyCommon,
    complaints: BTreeMap<AuthorityCount, Complaints6>,
    agg_pubkey: ValidAggregateKey<P>,
    // Blame responses received from other parties in the previous communication round
    blame_responses: BTreeMap<AuthorityCount, Option<BlameResponse8<P>>>,
    shares: IncomingShares<P>,
    commitments: BTreeMap<AuthorityCount, DKGCommitment<P>>,
}

derive_display_as_type_name!(VerifyBlameResponsesBroadcastStage9<P: ECPoint>);

/// Checks for sender_idx that their blame response contains exactly
/// a share for each party that blamed them
fn is_blame_response_complete<P: ECPoint>(
    sender_idx: AuthorityCount,
    response: &BlameResponse8<P>,
    complaints: &BTreeMap<AuthorityCount, Complaints6>,
) -> bool {
    let expected_idxs_iter = complaints
        .iter()
        .filter_map(|(blamer_idx, Complaints6(blamed_idxs))| {
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

impl<P: ECPoint> VerifyBlameResponsesBroadcastStage9<P> {
    /// Check that blame responses contain all (and only) the requested shares, and that all the shares are valid.
    /// If all responses are valid, returns shares destined for us along with the corresponding index. Otherwise,
    /// returns a list of party indexes who provided invalid responses.
    fn check_blame_responses(
        &self,
        blame_responses: BTreeMap<AuthorityCount, BlameResponse8<P>>,
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

#[async_trait]
impl<P: ECPoint> BroadcastStageProcessor<KeygenData<P>, KeygenResultInfo<P>, KeygenFailureReason>
    for VerifyBlameResponsesBroadcastStage9<P>
{
    type Message = VerifyBlameResponses9<P>;
    const NAME: CeremonyStageName = CeremonyStageName::VerifyBlameResponsesBroadcastStage9;

    fn init(&mut self) -> DataToSend<Self::Message> {
        let data = self.blame_responses.clone();

        DataToSend::Broadcast(VerifyBlameResponses9 { data })
    }

    async fn process(
        mut self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> KeygenStageResult<P> {
        slog::debug!(self.common.logger, "Processing {}", Self::NAME);

        let verified_responses = match verify_broadcasts(messages, &self.common.logger) {
            Ok(comms) => comms,
            Err((reported_parties, abort_reason)) => {
                return KeygenStageResult::Error(
                    reported_parties,
                    CeremonyFailureReason::BroadcastFailure(abort_reason, Self::NAME),
                );
            }
        };

        match self.check_blame_responses(verified_responses) {
            Ok(shares_for_us) => {
                for (sender_idx, share) in shares_for_us {
                    self.shares.0.insert(sender_idx, share);
                }

                finalize_keygen(self.common, self.agg_pubkey, self.shares, self.commitments).await
            }
            Err(bad_parties) => StageResult::Error(
                bad_parties,
                CeremonyFailureReason::Other(KeygenFailureReason::InvalidBlameResponse),
            ),
        }
    }
}
