use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use cf_traits::AuthorityCount;
use pallet_cf_vaults::CeremonyId;

use crate::{
    common::format_iterator,
    logging::CEREMONY_ID_KEY,
    multisig::client::common::{ProcessMessageResult, StageResult},
};
use state_chain_runtime::{constants::common::MAX_STAGE_DURATION_SECONDS, AccountId};

use super::{
    ceremony_manager::CeremonyResultSender,
    common::{CeremonyFailureReason, CeremonyStage},
    utils::PartyIdxMapping,
};

const MAX_STAGE_DURATION: Duration = Duration::from_secs(MAX_STAGE_DURATION_SECONDS as u64);

type OptionalCeremonyReturn<CeremonyResult, FailureReason> =
    Option<Result<CeremonyResult, (BTreeSet<AccountId>, CeremonyFailureReason<FailureReason>)>>;

pub struct StateAuthorised<CeremonyData, CeremonyResult, FailureReason> {
    pub stage: Option<
        Box<
            dyn CeremonyStage<
                    Message = CeremonyData,
                    Result = CeremonyResult,
                    FailureReason = FailureReason,
                > + Send,
        >,
    >,
    pub result_sender: CeremonyResultSender<CeremonyResult, FailureReason>,
    pub idx_mapping: Arc<PartyIdxMapping>,
    pub num_of_participants: AuthorityCount,
}

pub struct StateRunner<CeremonyData, CeremonyResult, FailureReason> {
    inner: Option<StateAuthorised<CeremonyData, CeremonyResult, FailureReason>>,
    // Note that we use a map here to limit the number of messages
    // that can be delayed from any one party to one per stage.
    delayed_messages: BTreeMap<AccountId, CeremonyData>,
    /// Time point at which the current ceremony is considered expired and gets aborted
    should_expire_at: std::time::Instant,
    logger: slog::Logger,
}

impl<CeremonyData, CeremonyResult, FailureReason>
    StateRunner<CeremonyData, CeremonyResult, FailureReason>
where
    CeremonyData: Display,
    FailureReason: Display,
{
    /// Create ceremony state without a ceremony request (which is expected to arrive
    /// shortly). Until such request is received, we can start delaying messages, but
    /// cannot make any progress otherwise
    pub fn new_unauthorised(ceremony_id: CeremonyId, logger: &slog::Logger) -> Self {
        StateRunner {
            inner: None,
            delayed_messages: Default::default(),
            should_expire_at: Instant::now() + MAX_STAGE_DURATION,
            logger: logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
        }
    }

    /// Process ceremony request from the State Chain, which allows
    /// the state machine to make progress
    pub fn on_ceremony_request(
        &mut self,
        mut stage: Box<
            dyn CeremonyStage<
                    Message = CeremonyData,
                    Result = CeremonyResult,
                    FailureReason = FailureReason,
                > + Send,
        >,
        idx_mapping: Arc<PartyIdxMapping>,
        result_sender: CeremonyResultSender<CeremonyResult, FailureReason>,
        num_of_participants: AuthorityCount,
    ) -> OptionalCeremonyReturn<CeremonyResult, FailureReason> {
        if self.inner.is_some() {
            let _result = result_sender.send(Err((
                BTreeSet::new(),
                CeremonyFailureReason::DuplicateCeremonyId,
            )));
            return None;
        }

        stage.init();

        self.inner = Some(StateAuthorised {
            stage: Some(stage),
            idx_mapping,
            result_sender,
            num_of_participants,
        });

        // Unlike other state transitions, we don't take into account
        // any time left in the prior stage when receiving a request
        // to sign (we don't want other parties to be able to
        // control when our stages time out)
        self.should_expire_at = Instant::now() + MAX_STAGE_DURATION;

        self.process_delayed()
    }

    fn finalize_current_stage(&mut self) -> OptionalCeremonyReturn<CeremonyResult, FailureReason> {
        // Ideally, we would pass the authorised state as a parameter
        // as it is always present (i.e. not `None`) when this function
        // is called, but the borrow checker won't let allow this.

        let authorised_state = self
            .inner
            .as_mut()
            .expect("Ceremony must be authorised to finalize any of its stages");

        let stage = authorised_state
            .stage
            .take()
            .expect("Stage must be present to be finalized");

        match stage.finalize(&self.logger) {
            StageResult::NextStage(mut next_stage) => {
                slog::debug!(
                    self.logger,
                    "Ceremony transitions to {}",
                    next_stage.get_stage_name()
                );

                next_stage.init();

                authorised_state.stage = Some(next_stage);

                // Instead of resetting the expiration time, we simply extend
                // it (any remaining time carries over to the next stage).
                // Doing it otherwise would allow other parties to influence
                // the time at which the stages in individual nodes time out
                // (by sending their data at specific times) thus making some
                // attacks possible.
                self.should_expire_at += MAX_STAGE_DURATION;

                self.process_delayed()
            }
            StageResult::Error(bad_validators, reason) => Some(Err((
                authorised_state.idx_mapping.get_ids(bad_validators),
                reason,
            ))),
            StageResult::Done(result) => {
                slog::debug!(self.logger, "Ceremony reached the final stage!");

                Some(Ok(result))
            }
        }
    }

    /// Process message from a peer, returning ceremony outcome if
    /// the ceremony stage machine cannot progress any further
    pub fn process_or_delay_message(
        &mut self,
        sender_id: AccountId,
        data: CeremonyData,
    ) -> OptionalCeremonyReturn<CeremonyResult, FailureReason> {
        match &mut self.inner {
            None => {
                self.add_delayed(sender_id, data);
            }
            Some(authorised_state) => {
                let stage = authorised_state.stage.as_mut().expect(
                    "The value is only None for a brief period of time, when we swap states, below",
                );

                // Check that the sender is a possible participant in the ceremony
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

                // Check if we should delay this message for the next stage to use
                if stage.should_delay(&data) {
                    self.add_delayed(sender_id, data);
                    return None;
                }

                if let ProcessMessageResult::Ready = stage.process_message(sender_idx, data) {
                    return self.finalize_current_stage();
                }
            }
        }

        None
    }

    /// Process previously delayed messages (which arrived one stage too early)
    pub fn process_delayed(&mut self) -> OptionalCeremonyReturn<CeremonyResult, FailureReason> {
        let messages = std::mem::take(&mut self.delayed_messages);

        for (id, m) in messages {
            slog::debug!(
                self.logger,
                "Processing delayed message {} from party [{}]",
                m,
                id,
            );

            if let Some(result) = self.process_or_delay_message(id, m) {
                return Some(result);
            }
        }

        None
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
                    stage.get_stage_name()
                );
            }
            None => {
                slog::debug!(
                    self.logger,
                    "Delaying message {} from party [{}] for unauthorised ceremony",
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

    /// Check if the stage has timed out, and if so, proceed according to the
    /// protocol rules for the stage
    pub fn try_expiring(&mut self) -> OptionalCeremonyReturn<CeremonyResult, FailureReason> {
        if self.should_expire_at < std::time::Instant::now() {
            match &self.inner {
                None => {
                    // Report the parties that tried to initiate the ceremony.
                    let reported_ids = self
                        .delayed_messages
                        .iter()
                        .map(|(id, _)| id.clone())
                        .collect();

                    slog::warn!(
                        self.logger,
                        "Ceremony expired before being authorized, reporting parties: {}",
                        format_iterator(&reported_ids)
                    );

                    Some(Err((
                        reported_ids,
                        CeremonyFailureReason::ExpiredBeforeBeingAuthorized,
                    )))
                }
                Some(_authorised_state) => {
                    // We can't simply abort here as we don't know whether other
                    // participants are going to do the same (e.g. if a malicious
                    // node targeted us by communicating with everyone but us, it
                    // would look to the rest of the network like we are the culprit).
                    // Instead, we delegate the responsibility to the concrete stage
                    // implementation to try to recover or agree on who to report.

                    slog::warn!(
                        self.logger,
                        "Ceremony stage timed out before all messages collected; trying to finalize current stage anyway"
                    );

                    self.finalize_current_stage()
                }
            }
        } else {
            None
        }
    }

    /// returns true if the ceremony is authorized (has received a ceremony request)
    pub fn is_authorized(&self) -> bool {
        self.inner.is_some()
    }

    /// Returns the number of participants in the current ceremony
    pub fn get_participant_count(&self) -> Option<AuthorityCount> {
        self.inner
            .as_ref()
            .map(|authorised_state| authorised_state.num_of_participants)
    }

    pub fn try_into_result_sender(
        self,
    ) -> Option<CeremonyResultSender<CeremonyResult, FailureReason>> {
        self.inner.map(|inner| inner.result_sender)
    }

    #[cfg(test)]
    pub fn get_stage_name(&self) -> Option<super::common::CeremonyStageName> {
        self.inner
            .as_ref()
            .and_then(|s| s.stage.as_ref().map(|s| s.get_stage_name()))
    }

    #[cfg(test)]
    pub fn get_awaited_parties_count(&self) -> Option<AuthorityCount> {
        self.inner.as_ref().and_then(|s| {
            s.stage
                .as_ref()
                .map(|s| s.awaited_parties().len() as AuthorityCount)
        })
    }

    #[cfg(test)]
    pub fn set_expiry_time(&mut self, expiry_time: std::time::Instant) {
        self.should_expire_at = expiry_time;
    }

    #[cfg(test)]
    pub fn get_delayed_messages_len(&self) -> usize {
        self.delayed_messages.len()
    }
}

#[cfg(test)]
impl<CeremonyData, CeremonyResult, FailureReason>
    StateRunner<CeremonyData, CeremonyResult, FailureReason>
where
    CeremonyData: Display,
    FailureReason: Display,
{
    pub fn new_authorised(
        ceremony_id: CeremonyId,
        stage: Box<
            dyn CeremonyStage<
                    Message = CeremonyData,
                    Result = CeremonyResult,
                    FailureReason = FailureReason,
                > + Send,
        >,
        idx_mapping: Arc<PartyIdxMapping>,
        result_sender: CeremonyResultSender<CeremonyResult, FailureReason>,
        num_of_participants: AuthorityCount,
        logger: slog::Logger,
    ) -> Self {
        let inner = Some(StateAuthorised {
            stage: Some(stage),
            idx_mapping,
            result_sender,
            num_of_participants,
        });

        StateRunner {
            inner,
            delayed_messages: Default::default(),
            should_expire_at: Instant::now() + MAX_STAGE_DURATION,
            logger: logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
        }
    }
}
