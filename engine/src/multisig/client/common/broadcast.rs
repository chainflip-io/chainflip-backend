use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    marker::PhantomData,
};

use cf_traits::AuthorityCount;

use crate::{
    multisig::{
        client::{MultisigData, MultisigMessage},
        crypto::ECPoint,
    },
    multisig_p2p::OutgoingMultisigStageMessages,
};

use super::{
    ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult},
    BroadcastStageName,
};

pub use super::broadcast_verification::verify_broadcasts;

/// Used by individual stages to distinguish between
/// a public message that should be broadcast to everyone
/// an secret messages that should be delivered to different
/// parties in private
pub enum DataToSend<T> {
    Broadcast(T),
    Private(BTreeMap<AuthorityCount, T>),
}

/// Abstracts away computations performed during every "broadcast" stage
/// of a ceremony
pub trait BroadcastStageProcessor<Data, Result, FailureReason>: Display {
    /// The specific variant of D shared between parties
    /// during this stage
    type Message: Clone + Into<Data> + TryFrom<Data>;

    /// Broadcast Stage Name used for logging.
    /// A broadcast and its verification will share the same name.
    const NAME: BroadcastStageName;

    /// Init the stage, returning the data to broadcast
    fn init(&mut self) -> DataToSend<Self::Message>;

    /// For a given message, signal if it needs to be delayed
    /// until the next stage
    fn should_delay(&self, m: &Data) -> bool;

    /// Determines how the data for this stage (of type `Self::Message`)
    /// should be processed once it either received it from all other parties
    /// or the stage timed out (None is used for missing messages)
    fn process(
        self,
        messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
    ) -> StageResult<Data, Result, FailureReason>;
}

/// Responsible for broadcasting/collecting of stage data,
/// delegating the actual processing to `StageProcessor`
pub struct BroadcastStage<Data, Result, Stage, Point, FailureReason>
where
    Stage: BroadcastStageProcessor<Data, Result, FailureReason>,
    Point: ECPoint,
{
    common: CeremonyCommon,
    /// Messages collected so far
    messages: BTreeMap<AuthorityCount, Stage::Message>,
    /// Determines the actual computations before/after
    /// the data is collected
    processor: Stage,
    _phantom: PhantomData<Point>,
}

impl<Data, Result, Stage, Point, FailureReason>
    BroadcastStage<Data, Result, Stage, Point, FailureReason>
where
    Data: Clone,
    Point: ECPoint,
    Stage: BroadcastStageProcessor<Data, Result, FailureReason>,
{
    pub fn new(processor: Stage, common: CeremonyCommon) -> Self {
        BroadcastStage {
            common,
            messages: BTreeMap::new(),
            processor,
            _phantom: Default::default(),
        }
    }
}

impl<Data, Result, Stage, Point, FailureReason> Display
    for BroadcastStage<Data, Result, Stage, Point, FailureReason>
where
    Stage: BroadcastStageProcessor<Data, Result, FailureReason>,
    Point: ECPoint,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BroadcastStage<{}>", &self.processor)
    }
}

impl<Point, Data, Result, Stage, FailureReason> CeremonyStage
    for BroadcastStage<Data, Result, Stage, Point, FailureReason>
where
    Point: ECPoint,
    Data: Clone + Display + Into<MultisigData<Point>>,
    Result: Clone,
    Stage: BroadcastStageProcessor<Data, Result, FailureReason>,
    <Stage as BroadcastStageProcessor<Data, Result, FailureReason>>::Message: TryFrom<Data>,
{
    type Message = Data;
    type Result = Result;
    type FailureReason = FailureReason;

    fn init(&mut self) {
        let common = &self.common;

        let idx_to_id = |idx: &AuthorityCount| {
            common
                .validator_mapping
                .get_id(*idx)
                .expect("Unknown account index")
                .clone()
        };

        let (own_message, outgoing_messages) = match self.processor.init() {
            DataToSend::Broadcast(stage_data) => {
                let ceremony_data: Data = stage_data.clone().into();
                (
                    stage_data,
                    OutgoingMultisigStageMessages::Broadcast(
                        common
                            .all_idxs
                            .iter()
                            .filter(|idx| **idx != common.own_idx)
                            .map(idx_to_id)
                            .collect(),
                        bincode::serialize(&MultisigMessage {
                            ceremony_id: common.ceremony_id,
                            data: ceremony_data.into(),
                        })
                        .unwrap(),
                    ),
                )
            }
            DataToSend::Private(mut messages) => (
                messages
                    .remove(&common.own_idx)
                    .expect("Must include message to self"),
                OutgoingMultisigStageMessages::Private(
                    messages
                        .into_iter()
                        .map(|(idx, stage_data)| {
                            let ceremony_data: Data = stage_data.into();
                            (
                                idx_to_id(&idx),
                                bincode::serialize(&MultisigMessage {
                                    ceremony_id: common.ceremony_id,
                                    data: ceremony_data.into(),
                                })
                                .unwrap(),
                            )
                        })
                        .collect(),
                ),
            ),
        };

        // Save our own share
        self.messages.insert(common.own_idx, own_message);

        self.common
            .outgoing_p2p_message_sender
            .send(outgoing_messages)
            .expect("Could not send p2p message.");
    }

    fn process_message(&mut self, signer_idx: AuthorityCount, m: Data) -> ProcessMessageResult {
        let m: Stage::Message = match m.try_into() {
            Ok(m) => m,
            Err(_) => {
                slog::warn!(
                    self.common.logger,
                    "Ignoring an unexpected message for stage {} from party [{}]",
                    self,
                    signer_idx
                );
                return ProcessMessageResult::NotReady;
            }
        };

        if self.messages.contains_key(&signer_idx) {
            slog::warn!(
                self.common.logger,
                "Ignoring a redundant message for stage {} from party [{}]",
                self,
                signer_idx
            );
            return ProcessMessageResult::NotReady;
        }

        if !self.common.all_idxs.contains(&signer_idx) {
            slog::warn!(
                self.common.logger,
                "Ignoring a message from non-participant for stage {} from party [{}]",
                self,
                signer_idx
            );
            return ProcessMessageResult::NotReady;
        }

        self.messages.insert(signer_idx, m);

        if self.messages.len() == self.common.all_idxs.len() {
            ProcessMessageResult::Ready
        } else {
            ProcessMessageResult::NotReady
        }
    }

    fn should_delay(&self, m: &Data) -> bool {
        self.processor.should_delay(m)
    }

    fn finalize(mut self: Box<Self>) -> StageResult<Data, Result, FailureReason> {
        // Because we might want to finalize the stage before
        // all data has been received (e.g. due to a timeout),
        // we insert None for any missing data

        let mut received_messages = std::mem::take(&mut self.messages);

        // Turns values T into Option<T>, inserting `None` where
        // data hasn't been received for `idx`
        let messages: BTreeMap<_, _> = self
            .common
            .all_idxs
            .iter()
            .map(|idx| (*idx, received_messages.remove(idx)))
            .collect();

        self.processor.process(messages)
    }

    fn awaited_parties(&self) -> BTreeSet<AuthorityCount> {
        let mut awaited = BTreeSet::new();

        for idx in &self.common.all_idxs {
            if !self.messages.contains_key(idx) {
                awaited.insert(*idx);
            }
        }

        awaited
    }
}
