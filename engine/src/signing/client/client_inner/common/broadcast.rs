use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt::Display,
};

use super::ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};

/// Abstracts away computations performed during every "broadcast" stage
/// of a ceremony
pub trait StageProcessor<D, Result>: Clone + Display {
    /// The specific variant of D shared between parties
    /// during this stage
    type Message: Clone + Into<D> + TryFrom<D>;

    /// Init the stage, returning the data to broadcast
    fn init(&self) -> Self::Message;

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
pub struct BroadcastStage<D, Result, P, W>
where
    P: StageProcessor<D, Result>,
{
    common: CeremonyCommon,
    /// Messages collected so far
    messages: HashMap<usize, P::Message>,
    /// Determines how an individual p2p message is wrapped
    /// (signing/keygen need a different wrapper)
    wrapper: W,
    /// Determines the actual computations before/after
    /// the data is collected
    processor: P,
}

/// Determines how a message of type `Data` is wrapped
/// (and serialized)
pub trait MessageWrapper<Data>: Clone {
    fn wrap_and_serialize(&self, data: &Data) -> Vec<u8>;
}

impl<D, Result, P, W> BroadcastStage<D, Result, P, W>
where
    D: Clone,
    P: StageProcessor<D, Result>,
    W: MessageWrapper<D>,
{
    pub fn new(processor: P, common: CeremonyCommon, wrapper: W) -> Self {
        BroadcastStage {
            common,
            messages: HashMap::new(),
            processor,
            wrapper,
        }
    }

    fn broadcast(&self, data: impl Into<D> + Clone + Display) {
        // slog::info!(self.logger, "Broadcasting data: {}", &data);

        let data = self.wrapper.wrap_and_serialize(&data.into());

        for idx in &self.common.all_idxs {
            if *idx == self.common.own_idx {
                continue;
            }

            // slog::debug!(self.logger, "Sending data to [{}]", idx);
            self.common.p2p_sender.send(*idx, data.clone());
        }
    }
}

impl<D, Result, P, W> Display for BroadcastStage<D, Result, P, W>
where
    P: StageProcessor<D, Result>,
    W: MessageWrapper<D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BroadcastStage<{}>", &self.processor)
    }
}

impl<D, Result, P, W> CeremonyStage for BroadcastStage<D, Result, P, W>
where
    D: Clone + Display,
    Result: Clone,
    P: StageProcessor<D, Result>,
    <P as StageProcessor<D, Result>>::Message: TryFrom<D>,
    W: MessageWrapper<D>,
{
    type Message = D;
    type Result = Result;

    fn init(&mut self) {
        let data = self.processor.init();

        // Save our own share
        self.messages.insert(self.common.own_idx, data.clone());

        self.broadcast(data.into());
    }

    fn process_message(&mut self, signer_idx: usize, m: D) -> ProcessMessageResult {
        let m: P::Message = match m.try_into() {
            Ok(m) => m,
            Err(_) => {
                eprintln!("Unexpected message type for stage {}", self);
                return ProcessMessageResult::Ignored;
            }
        };

        if self.messages.contains_key(&signer_idx) {
            // slog::error!(self.common.logger, "Already received from this party");

            println!("Already received from this party");
            return ProcessMessageResult::Ignored;
        }

        self.messages.insert(signer_idx, m);

        if self.messages.len() == self.common.all_idxs.len() {
            ProcessMessageResult::CollectedAll
        } else {
            ProcessMessageResult::Progress
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
