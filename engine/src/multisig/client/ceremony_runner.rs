#[cfg(test)]
mod ceremony_runner_tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use cf_traits::AuthorityCount;
use futures::future::{BoxFuture, FutureExt};
use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    common::format_iterator,
    logging::CEREMONY_ID_KEY,
    multisig::client::common::{ProcessMessageResult, StageResult},
};
use state_chain_runtime::{constants::common::MAX_STAGE_DURATION_SECONDS, AccountId};

use super::{
    ceremony_manager::{CeremonyRequestInner, CeremonyTrait, DynStage},
    common::{CeremonyFailureReason, PreProcessStageDataCheck},
    utils::PartyIdxMapping,
};

const MAX_STAGE_DURATION: Duration = Duration::from_secs(MAX_STAGE_DURATION_SECONDS as u64);

type OptionalCeremonyReturn<CeremonyResult, FailureReason> =
    Option<Result<CeremonyResult, (BTreeSet<AccountId>, CeremonyFailureReason<FailureReason>)>>;

pub struct StateAuthorised<Ceremony: CeremonyTrait> {
    pub stage: Option<DynStage<Ceremony>>,
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
    /// Returns the id of the ceremony to make it easier to identify
    /// which ceremony is finished when many are running
    pub async fn run(
        ceremony_id: CeremonyId,
        mut message_receiver: UnboundedReceiver<(AccountId, Ceremony::Data)>,
        mut request_receiver: UnboundedReceiver<CeremonyRequestInner<Ceremony>>,
        logger: slog::Logger,
    ) -> CeremonyId {
        // We always create unauthorised first, it can get promoted to
        // an authorised one with a ceremony request
        let mut runner = Self::new_unauthorised(ceremony_id, &logger);
        let mut final_result_sender = None;

        let outcome = loop {
            tokio::select! {
                Some((sender_id, message)) = message_receiver.recv() => {

                    if let Some(res) = runner.process_or_delay_message(sender_id, message).await {
                        break res;
                    }

                }
                Some(request) = request_receiver.recv() => {

                    let CeremonyRequestInner { init_stage, idx_mapping, result_sender, num_of_participants } = request;
                    final_result_sender = Some(result_sender);

                    if let Some(res) = runner.on_ceremony_request(init_stage, idx_mapping, num_of_participants).await {
                        break res;
                    }

                }
                () = runner.sleep_handle.as_mut() => {

                    if let Some(res) = runner.on_timeout().await {
                        break res;
                    }

                }
            }
        };

        if let Some(result_sender) = final_result_sender {
            let _res = result_sender.send(outcome);
        }

        ceremony_id
    }

    /// Create ceremony state without a ceremony request (which is expected to arrive
    /// shortly). Until such request is received, we can start delaying messages, but
    /// cannot make any progress otherwise
    fn new_unauthorised(ceremony_id: CeremonyId, logger: &slog::Logger) -> Self {
        let sleep_handle = Box::pin(tokio::time::sleep(MAX_STAGE_DURATION));
        CeremonyRunner {
            inner: None,
            delayed_messages: Default::default(),
            sleep_handle,
            logger: logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
        }
    }

    /// Process ceremony request from the State Chain, which allows
    /// the state machine to make progress
    pub async fn on_ceremony_request(
        &mut self,
        mut stage: DynStage<Ceremony>,
        idx_mapping: Arc<PartyIdxMapping>,
        num_of_participants: AuthorityCount,
    ) -> OptionalCeremonyReturn<Ceremony::Output, Ceremony::FailureReason> {
        assert!(self.inner.is_none(), "Duplicate ceremony id");

        stage.init();

        self.inner = Some(StateAuthorised {
            stage: Some(stage),
            idx_mapping,
            num_of_participants,
        });

        // Unlike other state transitions, we don't take into account
        // any time left in the prior stage when receiving a ceremony request.
        // we don't want other parties to be able to control when our stages time out.
        self.sleep_handle
            .as_mut()
            .reset(tokio::time::Instant::now() + MAX_STAGE_DURATION);

        self.process_delayed().await
    }

    async fn finalize_current_stage(
        &mut self,
    ) -> OptionalCeremonyReturn<Ceremony::Output, Ceremony::FailureReason> {
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

        match stage.finalize().await {
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
    ) -> OptionalCeremonyReturn<Ceremony::Output, Ceremony::FailureReason> {
        match &mut self.inner {
            None => {
                if !data.is_first_stage() {
                    slog::debug!(
                        self.logger,
                        "Ignoring data: non-initial stage data for unauthorised ceremony";
                        "from_id" => sender_id.to_string(),
                    );
                    return None;
                }

                // We do not need to check data_size_is_valid here because stage 1 messages are always the correct size.

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

                // Check that the number of elements in the data is what we expect
                if !data.data_size_is_valid(authorised_state.num_of_participants) {
                    slog::debug!(
                        self.logger,
                        "Ignoring data: incorrect number of elements";
                        "from_id" => sender_id.to_string(),
                    );
                    return None;
                }

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
    fn process_delayed(
        &mut self,
    ) -> BoxFuture<OptionalCeremonyReturn<Ceremony::Output, Ceremony::FailureReason>> {
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

    async fn on_timeout(
        &mut self,
    ) -> OptionalCeremonyReturn<Ceremony::Output, Ceremony::FailureReason> {
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
            Some(authorised_state) => {
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

                // Log the account ids of the missing messages
                if let Some(stage) = &authorised_state.stage {
                    let missing_messages_from_accounts = authorised_state
                        .idx_mapping
                        .get_ids(stage.awaited_parties());
                    slog::debug!(
                        self.logger,
                        "Stage `{}` is missing messages from {} parties",
                        stage.get_stage_name(),
                        missing_messages_from_accounts.len();
                        "missing_ids" => format_iterator(missing_messages_from_accounts).to_string()
                    )
                }

                self.finalize_current_stage().await
            }
        }
    }
}

#[cfg(test)]
impl<Ceremony: CeremonyTrait> CeremonyRunner<Ceremony> {
    /// This is to allow calling a private method from tests
    pub fn new_unauthorised_for_test(ceremony_id: CeremonyId, logger: &slog::Logger) -> Self {
        Self::new_unauthorised(ceremony_id, logger)
    }

    pub fn new_authorised(
        ceremony_id: CeremonyId,
        stage: DynStage<Ceremony>,
        idx_mapping: Arc<PartyIdxMapping>,
        num_of_participants: AuthorityCount,
        logger: slog::Logger,
    ) -> Self {
        let inner = Some(StateAuthorised {
            stage: Some(stage),
            idx_mapping,
            num_of_participants,
        });

        CeremonyRunner {
            inner,
            delayed_messages: Default::default(),
            sleep_handle: Box::pin(tokio::time::sleep(MAX_STAGE_DURATION)),
            logger: logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
        }
    }

    pub fn get_awaited_parties_count(&self) -> Option<AuthorityCount> {
        self.inner.as_ref().and_then(|s| {
            s.stage
                .as_ref()
                .map(|s| s.awaited_parties().len() as AuthorityCount)
        })
    }

    pub async fn force_timeout(
        &mut self,
    ) -> OptionalCeremonyReturn<Ceremony::Output, Ceremony::FailureReason> {
        self.on_timeout().await
    }

    pub fn get_stage_name(&self) -> Option<super::common::CeremonyStageName> {
        self.inner
            .as_ref()
            .and_then(|s| s.stage.as_ref().map(|s| s.get_stage_name()))
    }
}
