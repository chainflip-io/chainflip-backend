use std::{
    fmt::Display,
    sync::Arc,
    time::{Duration, Instant},
};

use pallet_cf_vaults::CeremonyId;

use crate::{
    multisig::client::common::{ProcessMessageResult, StageResult},
    p2p::AccountId,
};

use super::{common::CeremonyStage, utils::PartyIdxMapping, EventSender};

const MAX_STAGE_DURATION: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct StateAuthorised<CeremonyData, CeremonyResult>
where
    Box<dyn CeremonyStage<Message = CeremonyData, Result = CeremonyResult>>: Clone,
{
    pub ceremony_id: CeremonyId,
    pub stage: Option<Box<dyn CeremonyStage<Message = CeremonyData, Result = CeremonyResult>>>,
    pub result_sender: EventSender,
    pub idx_mapping: Arc<PartyIdxMapping>,
}

#[derive(Clone)]
pub struct StateRunner<CeremonyData, CeremonyResult>
where
    Box<dyn CeremonyStage<Message = CeremonyData, Result = CeremonyResult>>: Clone,
{
    logger: slog::Logger,
    inner: Option<StateAuthorised<CeremonyData, CeremonyResult>>,
    delayed_messages: Vec<(AccountId, CeremonyData)>,
    /// Time point at which the current ceremony is considered expired and gets aborted
    should_expire_at: std::time::Instant,
}

impl<CeremonyData, CeremonyResult> StateRunner<CeremonyData, CeremonyResult>
where
    CeremonyData: Display,
    Box<dyn CeremonyStage<Message = CeremonyData, Result = CeremonyResult>>: Clone,
{
    /// Create ceremony state without a ceremony request (which is expected to arrive
    /// shortly). Until such request is received, we can start delaying messages, but
    /// cannot make any progress otherwise
    pub fn new_unauthorised(logger: &slog::Logger) -> Self {
        StateRunner {
            inner: None,
            delayed_messages: Default::default(),
            should_expire_at: Instant::now() + MAX_STAGE_DURATION,
            logger: logger.clone(),
        }
    }

    /// Process ceremony request from the State Chain, which allows
    /// the state machine to make progress
    pub fn on_ceremony_request(
        &mut self,
        ceremony_id: CeremonyId,
        mut stage: Box<dyn CeremonyStage<Message = CeremonyData, Result = CeremonyResult>>,
        idx_mapping: Arc<PartyIdxMapping>,
        result_sender: EventSender,
    ) {
        if self.inner.is_some() {
            slog::warn!(
                self.logger,
                "Request to sign ignored: duplicate ceremony_id"
            );
            return;
        }

        stage.init();

        self.inner = Some(StateAuthorised {
            ceremony_id,
            stage: Some(stage),
            idx_mapping,
            result_sender,
        });

        // Unlike other state transitions, we don't take into account
        // any time left in the prior stage when receiving a request
        // to sign (we don't want other parties to be able to
        // control when our stages time out)
        self.should_expire_at = Instant::now() + MAX_STAGE_DURATION;

        self.process_delayed();
    }

    /// Process message from a peer, returning ceremony outcome if
    /// the ceremony stage machine cannot progress any further
    pub fn process_message(
        &mut self,
        sender_id: AccountId,
        data: CeremonyData,
    ) -> Option<Result<CeremonyResult, Vec<AccountId>>> {
        slog::trace!(
            self.logger,
            "Received message {} from party [{}] ",
            data,
            sender_id
        );

        match &mut self.inner {
            None => {
                self.add_delayed(sender_id, data);
            }
            Some(authorised_state) => {
                let stage = authorised_state.stage.as_mut().expect(
                    "The value is only None for a brief period of time, when we swap states, below",
                );

                if stage.should_delay(&data) {
                    self.add_delayed(sender_id, data);
                    return None;
                }

                // Check that the sender is a participant in the ceremony
                let sender_idx = match authorised_state.idx_mapping.get_idx(&sender_id) {
                    Some(idx) => idx,
                    None => {
                        slog::debug!(
                            self.logger,
                            "Sender {} is not a valid participant",
                            sender_id
                        );
                        return None;
                    }
                };

                if let ProcessMessageResult::Ready = stage.process_message(sender_idx, data) {
                    let stage = authorised_state.stage.take().unwrap();

                    match stage.finalize() {
                        StageResult::NextStage(mut next_stage) => {
                            slog::debug!(self.logger, "Ceremony transitions to {}", &next_stage);

                            next_stage.init();

                            authorised_state.stage = Some(next_stage);

                            // Instead of resetting the expiration time, we simply extend
                            // it (any remaining time carries over to the next stage).
                            // Doing it otherwise would allow other parties to influence
                            // when stages in individual nodes time out (by sending their
                            // data at specific times) thus making some attacks possible.
                            self.should_expire_at += MAX_STAGE_DURATION;

                            self.process_delayed();
                        }
                        StageResult::Error(bad_validators) => {
                            return Some(Err(authorised_state.idx_mapping.get_ids(bad_validators)));
                        }
                        StageResult::Done(result) => {
                            slog::debug!(self.logger, "Ceremony reached the final stage!");

                            return Some(Ok(result));
                        }
                    }
                }
            }
        }

        None
    }

    /// Process previously delayed messages (which arrived one stage too early)
    pub fn process_delayed(&mut self) {
        let messages = std::mem::take(&mut self.delayed_messages);

        for (id, m) in messages {
            slog::debug!(
                self.logger,
                "Processing delayed message {} from party [{}]",
                m,
                id,
            );
            self.process_message(id, m);
        }
    }

    /// Delay message to be processed in the next stage
    fn add_delayed(&mut self, id: AccountId, m: CeremonyData) {
        match &self.inner {
            Some(authorised_state) => {
                let stage = authorised_state
                    .stage
                    .as_ref()
                    .expect("stage should always exist");
                slog::debug!(
                    self.logger,
                    "Delaying message {} from party [{}] during stage: {}",
                    m,
                    id,
                    stage
                );
            }
            None => {
                slog::debug!(
                    self.logger,
                    "Delaying message {} from party [{}] (pre signing request)",
                    m,
                    id
                )
            }
        }

        self.delayed_messages.push((id, m));
    }

    /// Check if the state expired, and if so, return the parties that
    /// haven't submitted data for the current stage
    pub fn try_expiring(&self) -> Option<Vec<AccountId>> {
        if self.should_expire_at < std::time::Instant::now() {
            match &self.inner {
                None => {
                    // blame the parties that tried to initiate the ceremony
                    let blamed_ids = self
                        .delayed_messages
                        .iter()
                        .map(|(id, _)| id.clone())
                        .collect();

                    slog::warn!(
                        self.logger,
                        "Keygen ceremony expired before a request to sign, blaming parties: {:?}",
                        blamed_ids
                    );

                    Some(blamed_ids)
                }
                Some(authorised_state) => {
                    // blame slow parties
                    let blamed_idxs = authorised_state
                        .stage
                        .as_ref()
                        .expect("stage in authorised state is always present")
                        .awaited_parties();

                    let blamed_ids = authorised_state.idx_mapping.get_ids(blamed_idxs);

                    slog::warn!(
                        self.logger,
                        "Keygen ceremony expired, blaming parties: {:?}",
                        blamed_ids,
                    );

                    Some(blamed_ids)
                }
            }
        } else {
            None
        }
    }

    #[cfg(test)]
    pub fn get_stage(&self) -> Option<String> {
        self.inner
            .as_ref()
            .and_then(|s| s.stage.as_ref().map(|s| s.to_string()))
    }

    #[cfg(test)]
    pub fn set_expiry_time(&mut self, expiry_time: std::time::Instant) {
        self.should_expire_at = expiry_time;
    }
}
