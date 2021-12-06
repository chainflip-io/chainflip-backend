use std::{
    collections::BTreeMap,
    fmt::Display,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use pallet_cf_vaults::CeremonyId;
use tokio::sync::oneshot;

use crate::{
    multisig::client::common::{ProcessMessageResult, StageResult},
    p2p::AccountId,
};

use super::{CeremonyAbortReason, MultisigOutcomeSender, common::CeremonyStage, utils::PartyIdxMapping};

const MAX_STAGE_DURATION: Duration = Duration::from_secs(15);

pub struct StateAuthorised<CeremonyData, CeremonyResult>
where
    Box<dyn CeremonyStage<Message = CeremonyData, Result = CeremonyResult>>: Clone,
{
    pub ceremony_id: CeremonyId,
    pub stage: Option<Box<dyn CeremonyStage<Message = CeremonyData, Result = CeremonyResult>>>,
    pub result_sender: oneshot::Sender<Result<CeremonyResult, (CeremonyAbortReason, Vec<AccountId>)>>,
    pub idx_mapping: Arc<PartyIdxMapping>,
}

pub struct StateRunner<CeremonyData, CeremonyResult>
where
    Box<dyn CeremonyStage<Message = CeremonyData, Result = CeremonyResult>>: Clone,
{
    logger: slog::Logger,
    inner: Option<StateAuthorised<CeremonyData, CeremonyResult>>,
    // Note that we use a map here to limit the number of messages
    // that can be delayed from any one party to one per stage.
    delayed_messages: BTreeMap<AccountId, CeremonyData>,
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
        result_sender: oneshot::Sender<Result<CeremonyResult, (CeremonyAbortReason, Vec<AccountId>)>>,
    ) -> Result<()> {
        if self.inner.is_some() {
            return Err(anyhow::Error::msg("Duplicate ceremony_id"));
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

        Ok(())
    }

    /// Process message from a peer, returning ceremony outcome if
    /// the ceremony stage machine cannot progress any further
    pub fn process_message(
        &mut self,
        sender_id: AccountId,
        data: CeremonyData,
    ) -> bool {
        slog::trace!(
            self.logger,
            "Received message {} from party [{}] ",
            data,
            sender_id
        );

        if let Some(result) = match &mut self.inner {
            None => {
                self.add_delayed(sender_id, data);
                None
            }
            Some(authorised_state) => {
                let stage = authorised_state.stage.as_mut().expect(
                    "The value is only None for a brief period of time, when we swap states, below",
                );

                // Check that the sender is a possible participant in the ceremony
                match authorised_state.idx_mapping.get_idx(&sender_id) {
                    Some(idx) => {
                        // Check if we should delay this message for the next stage to use
                        if stage.should_delay(&data) {
                            self.add_delayed(sender_id, data);
                            None
                        } else {
                            if let ProcessMessageResult::Ready = stage.process_message(idx, data) {
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
            
                                        None
                                    }
                                    StageResult::Error(bad_validators, reason) => {
                                        Some(Err((
                                            reason,
                                            authorised_state.idx_mapping.get_ids(bad_validators),
                                        )))
                                    }
                                    StageResult::Done(result) => {
                                        Some(Ok(result))
                                    }
                                }
                            } else {
                                None
                            }
                        }
                    },
                    None => {
                        slog::debug!(
                            self.logger,
                            "Sender {} is not a valid participant",
                            sender_id
                        );
                        None
                    }
                }
            }
        } {
            // TODO This take leaves this state runner in a bad state (As if the ceremony hadn't been started). This odd behaviour could cause people to introduce bugs, it can be avoided though via a refactor
            self.inner.take().unwrap().result_sender.send(result.map_err(|(_, blamed)| (CeremonyAbortReason::Invalid, blamed))); // TODO unwrap
            true
        } else {
            false
        }
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

        self.delayed_messages.insert(id, m);

        slog::debug!(
            self.logger,
            "Total delayed: {}",
            self.delayed_messages.len()
        );
    }

    /// Check if the state expired, and if so, return the parties that
    /// haven't submitted data for the current stage
    pub fn try_expiring(&self) -> Option<Vec<AccountId>> {
        if self.should_expire_at < std::time::Instant::now() {
            match &self.inner {
                None => {
                    // report the parties that tried to initiate the ceremony
                    let reported_ids = self
                        .delayed_messages
                        .iter()
                        .map(|(id, _)| id.clone())
                        .collect();

                    slog::warn!(
                        self.logger,
                        "Ceremony expired before being authorized, reporting parties: {:?}",
                        reported_ids
                    );

                    Some(reported_ids)
                }
                Some(authorised_state) => {
                    // report slow parties
                    let reported_idxs = authorised_state
                        .stage
                        .as_ref()
                        .expect("stage in authorised state is always present")
                        .awaited_parties();

                    let reported_ids = authorised_state.idx_mapping.get_ids(reported_idxs);

                    slog::warn!(
                        self.logger,
                        "Ceremony expired, reporting parties: {:?}",
                        reported_ids,
                    );

                    Some(reported_ids)
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
