use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use cf_traits::AuthorityCount;
use futures::future::{BoxFuture, FutureExt};
use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    common::format_iterator,
    logging::CEREMONY_ID_KEY,
    multisig::client::common::{ProcessMessageResult, StageResult},
};
use state_chain_runtime::{constants::common::MAX_STAGE_DURATION_SECONDS, AccountId};

use super::{
    ceremony_manager::{CeremonyRequestInner, CeremonyResultSender, CeremonyTrait, DynStage},
    common::CeremonyFailureReason,
    utils::PartyIdxMapping,
};

const MAX_STAGE_DURATION: Duration = Duration::from_secs(MAX_STAGE_DURATION_SECONDS as u64);

type OptionalCeremonyReturn<CeremonyResult, FailureReason> =
    Option<Result<CeremonyResult, (BTreeSet<AccountId>, CeremonyFailureReason<FailureReason>)>>;

pub struct StateAuthorised<Ceremony: CeremonyTrait> {
    pub stage: Option<DynStage<Ceremony>>,
    pub result_sender: CeremonyResultSender<Ceremony>,
    pub idx_mapping: Arc<PartyIdxMapping>,
    pub num_of_participants: AuthorityCount,
}

pub struct CeremonyRunner<Ceremony: CeremonyTrait> {
    inner: Option<StateAuthorised<Ceremony>>,
    // Note that we use a map here to limit the number of messages
    // that can be delayed from any one party to one per stage.
    delayed_messages: BTreeMap<AccountId, Ceremony::Data>,
    /// This will fire on stage timeout
    sleep_handle: Pin<Box<tokio::time::Sleep>>,
    logger: slog::Logger,
}

impl<Ceremony: CeremonyTrait> CeremonyRunner<Ceremony> {
    /// Listen for requests until the ceremony is finished
    pub async fn run(
        ceremony_id: CeremonyId,
        mut message_receiver: UnboundedReceiver<(AccountId, Ceremony::Data)>,
        mut request_receiver: UnboundedReceiver<CeremonyRequestInner<Ceremony>>,
        outcome_sender: UnboundedSender<()>,
        logger: slog::Logger,
    ) {
        // We always create unauthorised first, it can get promoted to
        // an authorised one with a ceremony request
        let mut runner = Self::new_unauthorised(ceremony_id, &logger);

        let outcome = loop {
            tokio::select! {
                Some((sender_id, message)) = message_receiver.recv() => {

                    if let Some(res) = runner.process_or_delay_message(sender_id, message).await {
                        break res;
                    }

                }
                Some(request) = request_receiver.recv() => {

                    let CeremonyRequestInner { init_stage, idx_mapping, result_sender, num_of_participants } = request;

                    // If we already have an authorised ceremony, we need to be careful that
                    // a second request does not interfere with it

                    if let Some(res) = runner.on_ceremony_request(init_stage, idx_mapping, result_sender, num_of_participants).await {
                        break res;
                    }

                }
                () = runner.sleep_handle.as_mut() => {

                    if let Some(res) = runner.on_timeout().await {
                        break res;
                    }

                }
                else => {
                    // TODO: remove this branch once I'm confident that we can never get here
                    return;
                }
            }
        };

        // MAXIM: instead of a channel I should be able to use JoinHandle
        let _ = outcome_sender.send(());
        // We should always have inner state if we are reporting result
        let _ = runner.inner.unwrap().result_sender.send(outcome);
    }

    /// Create ceremony state without a ceremony request (which is expected to arrive
    /// shortly). Until such request is received, we can start delaying messages, but
    /// cannot make any progress otherwise
    fn new_unauthorised(ceremony_id: CeremonyId, logger: &slog::Logger) -> Self {
        // MAXIM: maybe unauthorised ceremonies should not expire?
        let sleep_handle = Box::pin(tokio::time::sleep(MAX_STAGE_DURATION));
        CeremonyRunner {
            inner: None,
            delayed_messages: Default::default(),
            sleep_handle,
            logger: logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
        }
    }

    /// This is to allow calling a private method from tests
    #[cfg(test)]
    pub fn new_unauthorised_for_test(ceremony_id: CeremonyId, logger: &slog::Logger) -> Self {
        Self::new_unauthorised(ceremony_id, logger)
    }

    /// Process ceremony request from the State Chain, which allows
    /// the state machine to make progress
    pub async fn on_ceremony_request(
        &mut self,
        mut stage: DynStage<Ceremony>,
        idx_mapping: Arc<PartyIdxMapping>,
        // MAXIM: change this to Sender<Outcome>?
        result_sender: CeremonyResultSender<Ceremony>,
        num_of_participants: AuthorityCount,
        // MAXIM: change this to Option<Outcome>
    ) -> OptionalCeremonyReturn<Ceremony::Artefact, Ceremony::FailureReason> {
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
        self.sleep_handle
            .as_mut()
            .reset(tokio::time::Instant::now() + MAX_STAGE_DURATION);

        self.process_delayed().await
    }

    async fn finalize_current_stage(
        &mut self,
    ) -> OptionalCeremonyReturn<Ceremony::Artefact, Ceremony::FailureReason> {
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

        match stage.finalize() {
            StageResult::NextStage(mut next_stage) => {
                slog::debug!(self.logger, "Ceremony transitions to {}", &next_stage);

                next_stage.init();

                authorised_state.stage = Some(next_stage);

                // Instead of resetting the expiration time, we simply extend
                // it (any remaining time carries over to the next stage).
                // Doing it otherwise would allow other parties to influence
                // the time at which the stages in individual nodes time out
                // (by sending their data at specific times) thus making some
                // attacks possible.
                {
                    let current_deadline = self.sleep_handle.as_ref().deadline();
                    self.sleep_handle
                        .as_mut()
                        .reset(current_deadline + MAX_STAGE_DURATION);
                }

                self.process_delayed().await
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
    pub async fn process_or_delay_message(
        &mut self,
        sender_id: AccountId,
        data: Ceremony::Data,
    ) -> OptionalCeremonyReturn<Ceremony::Artefact, Ceremony::FailureReason> {
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
                    return self.finalize_current_stage().await;
                }
            }
        }

        None
    }

    /// Process previously delayed messages (which arrived one stage too early)
    // NOTE: Need this boxed to help with async recursion
    pub fn process_delayed<'a>(
        &'a mut self,
    ) -> BoxFuture<'a, OptionalCeremonyReturn<Ceremony::Artefact, Ceremony::FailureReason>> {
        async {
            let messages = std::mem::take(&mut self.delayed_messages);

            for (id, m) in messages {
                slog::debug!(
                    self.logger,
                    "Processing delayed message {} from party [{}]",
                    m,
                    id,
                );

                if let Some(result) = self.process_or_delay_message(id, m).await {
                    return Some(result);
                }
            }

            None
        }
        .boxed()
    }

    /// Delay message to be processed in the next stage
    fn add_delayed(&mut self, id: AccountId, m: Ceremony::Data) {
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

    async fn on_timeout(
        &mut self,
    ) -> OptionalCeremonyReturn<Ceremony::Artefact, Ceremony::FailureReason> {
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

                self.finalize_current_stage().await
            }
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

    pub fn try_into_result_sender(self) -> Option<CeremonyResultSender<Ceremony>> {
        self.inner.map(|inner| inner.result_sender)
    }

    #[cfg(test)]
    pub fn get_stage(&self) -> Option<String> {
        self.inner
            .as_ref()
            .and_then(|s| s.stage.as_ref().map(|s| s.to_string()))
    }

    #[cfg(test)]
    pub fn get_awaited_parties_count(&self) -> Option<AuthorityCount> {
        self.inner.as_ref().and_then(|s| {
            s.stage
                .as_ref()
                .map(|s| s.awaited_parties().len() as AuthorityCount)
        })
    }
}

#[cfg(test)]
impl<Ceremony: CeremonyTrait> CeremonyRunner<Ceremony> {
    pub fn new_authorised(
        ceremony_id: CeremonyId,
        stage: DynStage<Ceremony>,
        idx_mapping: Arc<PartyIdxMapping>,
        result_sender: CeremonyResultSender<Ceremony>,
        num_of_participants: AuthorityCount,
        logger: slog::Logger,
    ) -> Self {
        let inner = Some(StateAuthorised {
            stage: Some(stage),
            idx_mapping,
            result_sender,
            num_of_participants,
        });

        CeremonyRunner {
            inner,
            delayed_messages: Default::default(),
            sleep_handle: Box::pin(tokio::time::sleep(MAX_STAGE_DURATION)),
            logger: logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
        }
    }
}
