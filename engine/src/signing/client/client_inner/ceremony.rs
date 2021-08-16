#![allow(dead_code)]

use crate::p2p::ValidatorId;
use std::{collections::HashMap, time::Instant};
use tokio::sync::mpsc::UnboundedSender;

pub enum CeremonyError<Error> {
    Unauthorised(Vec<ValidatorId>),
    Timeout(Vec<ValidatorId>),
    HandleMessageError(Error),
}

pub enum HandleMessageResult<Stage, Output, Error> {
    NoProgress,
    Progress(Option<Stage>),
    Complete(Result<Output, Error>),
}

pub trait CeremonyOutputStore<Id, Output> {
    fn get(&self, id: Id) -> Option<&Output>;
    fn set(&mut self, id: Id, output: Output);
}

pub trait CeremonyState<Id: Clone, Stage, Message, Output, Error> {
    fn try_handle_message(
        &mut self,
        id: &Id,
        stage: &Stage,
        validator_id: &ValidatorId,
        message: &Message,
    ) -> Option<HandleMessageResult<Stage, Output, Error>>;

    fn waiting_for_validators(&self, stage: &Stage) -> Vec<ValidatorId>;
}

struct Ceremony<Stage, State, Message> {
    option_last_progress_stage_state: Option<(Instant, Stage, State)>,
    delayed_messages: Vec<(ValidatorId, Message)>,
    last_delayed_message: Instant,
}
impl<Stage, State, Message> Ceremony<Stage, State, Message> {
    fn new(now: Instant) -> Self {
        Self {
            option_last_progress_stage_state: None,
            delayed_messages: vec![],
            last_delayed_message: now,
        }
    }
    fn add_delayed_message(&mut self, validator_id: ValidatorId, message: Message, now: Instant) {
        self.last_delayed_message = now;
        self.delayed_messages.push((validator_id, message));
    }
}

pub struct Ceremonies<
    Id: std::hash::Hash + Eq + Clone + std::fmt::Debug,
    Stage,
    Message,
    Output: Clone,
    Error,
    State: CeremonyState<Id, Stage, Message, Output, Error>,
    OutputStore: CeremonyOutputStore<Id, Output>,
> {
    ceremonys: HashMap<Id, Ceremony<Stage, State, Message>>,
    ceremony_outputs: OutputStore,
    completion_sender: UnboundedSender<(Id, Result<Output, CeremonyError<Error>>)>,
    logger: slog::Logger,
    _phantom: std::marker::PhantomData<Output>,
}
impl<
        Id: std::hash::Hash + Eq + Clone + std::fmt::Debug,
        Stage,
        Message,
        Output: Clone,
        Error,
        State: CeremonyState<Id, Stage, Message, Output, Error>,
        OutputStore: CeremonyOutputStore<Id, Output>,
    > Ceremonies<Id, Stage, Message, Output, Error, State, OutputStore>
{
    pub fn new(
        completion_sender: UnboundedSender<(Id, Result<Output, CeremonyError<Error>>)>,
        ceremony_outputs: OutputStore,
        logger: slog::Logger,
    ) -> Self {
        Self {
            ceremonys: Default::default(),
            ceremony_outputs,
            completion_sender,
            logger,
            _phantom: Default::default(),
        }
    }

    pub fn get_ceremony_output(&self, id: Id) -> Option<&Output> {
        self.ceremony_outputs.get(id)
    }

    pub fn start_ceremony(
        &mut self,
        id: Id,
        stage: Stage,
        state: State,
        now: Instant,
    ) -> Option<Output> {
        let ceremony = self.get_ceremony(id.clone(), now);
        // TODO: Why would we ever start duplicate ceremonys, I'll leave this soft check, but believe it should be changed to an assert!
        // assert!(ceremony.option_last_progress_stage_state.is_none());
        if let Some(_) = ceremony.option_last_progress_stage_state {
            slog::warn!(
                self.logger,
                "Ignoring a ceremony start for a previously started ceremony id: {:?}",
                id
            );
            None
        } else {
            let (_last_progress, stage, state) = ceremony
                .option_last_progress_stage_state
                .insert((now, stage, state));
            Self::handle_delayed_messages(&id, stage, state, &mut ceremony.delayed_messages)
                .and_then(|result| {
                    self.ceremony_completion(
                        id,
                        result.map_err(|err| CeremonyError::HandleMessageError(err)),
                    )
                })
        }
    }

    pub fn handle_message(
        &mut self,
        id: Id,
        validator_id: ValidatorId,
        message: Message,
        now: Instant,
    ) -> Option<Output> {
        {
            let ceremony = self.get_ceremony(id.clone(), now);
            if let Some((last_progress, stage, state)) =
                &mut ceremony.option_last_progress_stage_state
            {
                // TODO: Use match if let guard when stabilised
                use HandleMessageResult::{Complete, NoProgress, Progress};
                match state.try_handle_message(&id, stage, &validator_id, &message) {
                    Some(NoProgress) => None,
                    Some(Progress(new_stage)) => {
                        if let Some(new_stage) = new_stage {
                            *stage = new_stage;
                        }
                        *last_progress = now;
                        Self::handle_delayed_messages(
                            &id,
                            stage,
                            state,
                            &mut ceremony.delayed_messages,
                        )
                    }
                    Some(Complete(output)) => Some(output),
                    None => {
                        ceremony.add_delayed_message(validator_id, message, now);
                        None
                    }
                }
            } else {
                ceremony.add_delayed_message(validator_id, message, now);
                None
            }
        }
        .and_then(|result| {
            self.ceremony_completion(
                id,
                result.map_err(|err| CeremonyError::HandleMessageError(err)),
            )
        })
    }

    pub fn cleanup_inprogress_ceremonys(&mut self, cleanup_before: &Instant) {
        let mut completed_ceremonys = vec![];
        let logger = &self.logger;
        self.ceremonys.retain(|id, ceremony| {
            if let Some((last_progress, stage, state)) = &ceremony.option_last_progress_stage_state
            {
                // Remove all pending state that hasn't been updated since cleanup_before
                let cleanup = last_progress < cleanup_before;
                if cleanup {
                    slog::warn!(logger, "Ceremony state expired for id: {:?}", id);
                    completed_ceremonys.push((
                        id.clone(),
                        Err(CeremonyError::Timeout(state.waiting_for_validators(stage))),
                    ));
                }
                !cleanup
            } else {
                // Only cleanup old messages if ceremony hasn't been started
                let cleanup = &ceremony.last_delayed_message < cleanup_before;
                if cleanup {
                    // We never received a request for this ceremony, so any parties
                    // that tried to initiate a new ceremony are deemed malicious
                    slog::warn!(
                        logger,
                        "Ceremony expired w/o start request for id: {:?}",
                        id
                    );
                    completed_ceremonys.push((
                        id.clone(),
                        Err(CeremonyError::Unauthorised(
                            ceremony
                                .delayed_messages
                                .drain(..)
                                .map(|x| x.0)
                                .collect::<Vec<_>>(),
                        )),
                    ));
                }
                !cleanup
            }
        });
        for (id, result) in completed_ceremonys {
            self.ceremony_completion(id, result);
        }
    }

    fn handle_delayed_messages(
        id: &Id,
        stage: &mut Stage,
        state: &mut State,
        delayed_messages: &mut Vec<(ValidatorId, Message)>,
    ) -> Option<Result<Output, Error>> {
        // This code would be more efficient and clearer with an enum_map of stages to delayed messages?
        loop {
            use HandleMessageResult::{Complete, NoProgress, Progress};
            let mut delayed_msg_result = NoProgress;
            delayed_messages.retain(|(validator_id, message)| {
                if let Complete(_) = &delayed_msg_result {
                    // Don't try processing messages after completion
                    true
                } else if let Some(handle_msg_result) =
                    state.try_handle_message(id, stage, validator_id, &message)
                {
                    match handle_msg_result {
                        NoProgress => {}
                        Progress(new_stage) => {
                            if let Some(new_stage) = new_stage {
                                *stage = new_stage;
                            }
                            // Note how there is no need to update the last progress instant here.
                            delayed_msg_result = Progress(Some(())); // Avoid cloning the stage incase it stores some state
                        }
                        Complete(output) => {
                            delayed_msg_result = Complete(output);
                        }
                    }
                    false
                } else {
                    true
                }
            });
            match delayed_msg_result {
                Progress(_) => {}
                NoProgress => break None,
                Complete(result) => break Some(result),
            }
        }
    }

    fn ceremony_completion(
        &mut self,
        id: Id,
        result: Result<Output, CeremonyError<Error>>,
    ) -> Option<Output> {
        self.ceremonys.remove(&id);
        assert!(self.ceremony_outputs.get(id.clone()).is_none(), "Ceremony output already exists, should be impossible, CeremonyOutputStore impl is likely broken");
        let option_output = if let Ok(output) = &result {
            self.ceremony_outputs.set(id.clone(), output.clone());
            Some(output.clone())
        } else {
            None
        };
        if let Err(err) = self.completion_sender.send((id, result)) {
            slog::error!(self.logger, "Unable to send event, error: {}", err);
        }
        option_output
    }

    fn get_ceremony(&mut self, id: Id, now: Instant) -> &mut Ceremony<Stage, State, Message> {
        self.ceremonys
            .entry(id)
            .or_insert_with(|| Ceremony::new(now))
    }
}
