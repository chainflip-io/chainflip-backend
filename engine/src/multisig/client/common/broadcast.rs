use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt::Display,
};

use super::{
    ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult},
    P2PSender,
};

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
    fn init(&self) -> DataToSend<Self::Message>;

    /// For a given message, signal if it needs to be delayed
    /// until the next stage
    fn should_delay(&self, m: &D) -> bool;

    /// Determines how the data for this stage (of type `Self::Message`)
    /// should be processed once it is received from all other parties
    fn process(self, messages: HashMap<usize, Self::Message>) -> StageResult<D, Result>;
}

/// Responsible for broadcasting/collecting of stage data,
/// delegating the actual processing to `StageProcessor`
#[derive(Clone)]
pub struct BroadcastStage<D, Result, P, Sender>
where
    P: BroadcastStageProcessor<D, Result>,
    Sender: P2PSender<Data = D>,
{
    common: CeremonyCommon<D, Sender>,
    /// Messages collected so far
    messages: HashMap<usize, P::Message>,
    /// Determines the actual computations before/after
    /// the data is collected
    processor: P,
}

impl<D, Result, P, Sender> BroadcastStage<D, Result, P, Sender>
where
    D: Clone,
    P: BroadcastStageProcessor<D, Result>,
    Sender: P2PSender<Data = D>,
{
    pub fn new(processor: P, common: CeremonyCommon<D, Sender>) -> Self
    where
        Sender: P2PSender<Data = D>,
    {
        BroadcastStage {
            common,
            messages: HashMap::new(),
            processor,
        }
    }
}

impl<D, Result, P, Sender> Display for BroadcastStage<D, Result, P, Sender>
where
    P: BroadcastStageProcessor<D, Result>,
    Sender: P2PSender<Data = D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BroadcastStage<{}>", &self.processor)
    }
}

impl<D, Result, P, Sender> CeremonyStage for BroadcastStage<D, Result, P, Sender>
where
    D: Clone + Display,
    Result: Clone,
    P: BroadcastStageProcessor<D, Result>,
    <P as BroadcastStageProcessor<D, Result>>::Message: TryFrom<D>,
    Sender: P2PSender<Data = D>,
{
    type Message = D;
    type Result = Result;

    fn init(&mut self) {
        match self.processor.init() {
            DataToSend::Broadcast(data) => {
                for destination_idx in &self.common.all_idxs {
                    if *destination_idx == self.common.own_idx {
                        // Save our own share
                        self.messages.insert(self.common.own_idx, data.clone());
                    } else {
                        self.common
                            .p2p_sender
                            .send(*destination_idx, data.clone().into());
                    }
                }
            }
            DataToSend::Private(messages) => {
                for (destination_idx, data) in messages {
                    if destination_idx == self.common.own_idx {
                        self.messages.insert(self.common.own_idx, data);
                    } else {
                        self.common.p2p_sender.send(destination_idx, data.into());
                    }
                }
            }
        }
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

    fn finalize(self: Box<Self>) -> StageResult<D, Result> {
        self.processor.process(self.messages)
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
