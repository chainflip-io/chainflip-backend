use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt::Display,
};

use crate::{
    multisig::client::{MultisigData, MultisigMessage},
    multisig_p2p::OutgoingMultisigStageMessages,
};

use super::ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};

pub use super::broadcast_verification::verify_broadcasts;

/// Used by individual stages to distinguish between
/// a public message that should be broadcast to everyone
/// an secret messages that should be delivered to different
/// parties in private
pub enum DataToSend<T> {
    Broadcast(T),
    Private(HashMap<usize, T>),
}

/// Abstracts away computations performed during every "broadcast" stage
/// of a ceremony
pub trait BroadcastStageProcessor<D, Result>: Clone + Display {
    /// The specific variant of D shared between parties
    /// during this stage
    type Message: Clone + Into<D> + TryFrom<D>;

    /// Init the stage, returning the data to broadcast
    fn init(&mut self) -> DataToSend<Self::Message>;

    /// For a given message, signal if it needs to be delayed
    /// until the next stage
    fn should_delay(&self, m: &D) -> bool;

    /// Determines how the data for this stage (of type `Self::Message`)
    /// should be processed once it either received it from all other parties
    /// or the stage timed out (None is used for missing messages)
    fn process(self, messages: HashMap<usize, Option<Self::Message>>) -> StageResult<D, Result>;
}

/// Responsible for broadcasting/collecting of stage data,
/// delegating the actual processing to `StageProcessor`
#[derive(Clone)]
pub struct BroadcastStage<D, Result, P>
where
    P: BroadcastStageProcessor<D, Result>,
{
    common: CeremonyCommon,
    /// Messages collected so far
    messages: HashMap<usize, P::Message>,
    /// Determines the actual computations before/after
    /// the data is collected
    processor: P,
}

impl<D, Result, P> BroadcastStage<D, Result, P>
where
    D: Clone,
    P: BroadcastStageProcessor<D, Result>,
{
    pub fn new(processor: P, common: CeremonyCommon) -> Self {
        BroadcastStage {
            common,
            messages: HashMap::new(),
            processor,
        }
    }
}

impl<D, Result, P> Display for BroadcastStage<D, Result, P>
where
    P: BroadcastStageProcessor<D, Result>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BroadcastStage<{}>", &self.processor)
    }
}

impl<D, Result, P> CeremonyStage for BroadcastStage<D, Result, P>
where
    D: Clone + Display + Into<MultisigData>,
    Result: Clone,
    P: BroadcastStageProcessor<D, Result>,
    <P as BroadcastStageProcessor<D, Result>>::Message: TryFrom<D>,
{
    type Message = D;
    type Result = Result;

    fn init(&mut self) {
        let common = &self.common;

        let idx_to_id = |idx: &usize| {
            common
                .validator_mapping
                .get_id(*idx)
                .expect("Unknown account index")
                .clone()
        };

        let (own_message, outgoing_messages) = match self.processor.init() {
            DataToSend::Broadcast(stage_data) => {
                let ceremony_data: D = stage_data.clone().into();
                (
                    stage_data.clone(),
                    OutgoingMultisigStageMessages::Broadcast(
                        common
                            .all_idxs
                            .iter()
                            .filter(|idx| **idx != common.own_idx)
                            .map(idx_to_id)
                            .collect(),
                        MultisigMessage {
                            ceremony_id: common.ceremony_id,
                            data: ceremony_data.into(),
                        },
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
                            let ceremony_data: D = stage_data.into();
                            (
                                idx_to_id(&idx),
                                MultisigMessage {
                                    ceremony_id: common.ceremony_id,
                                    data: ceremony_data.into(),
                                },
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

    fn process_message(&mut self, signer_idx: usize, m: D) -> ProcessMessageResult {
        let m: P::Message = match m.try_into() {
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

    fn should_delay(&self, m: &D) -> bool {
        self.processor.should_delay(m)
    }

    fn finalize(mut self: Box<Self>) -> StageResult<D, Result> {
        // Because we might want to finalize the stage before
        // all data has been received (e.g. due to a timeout),
        // we insert None for any missing data

        let mut received_messages = std::mem::take(&mut self.messages);

        // Turns values T into Option<T>, inserting `None` where
        // data hasn't been received for `idx`
        let messages: HashMap<_, _> = self
            .common
            .all_idxs
            .iter()
            .map(|idx| (*idx, received_messages.remove(idx)))
            .collect();

        self.processor.process(messages)
    }

    fn awaited_parties(&self) -> Vec<usize> {
        let mut awaited = vec![];

        for idx in &self.common.all_idxs {
            if !self.messages.contains_key(idx) {
                awaited.push(*idx);
            }
        }

        awaited
    }
}
