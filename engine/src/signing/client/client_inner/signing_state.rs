use std::sync::Arc;
use std::time::{Duration, Instant};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;

use crate::logging::CEREMONY_ID_KEY;
use crate::p2p::AccountId;

use crate::signing::{MessageHash, SigningOutcome};

use super::client_inner::{CeremonyOutcomeResult, Error, EventSender, MultisigMessage};

use super::common::{
    broadcast::BroadcastStage, CeremonyCommon, CeremonyStage, KeygenResult, P2PSender,
    ProcessMessageResult, RawP2PSender, StageResult,
};

use super::frost::{SigningData, SigningDataWrapped};
use super::utils::ValidatorMaps;
use super::{InnerEvent, KeygenResultInfo, SchnorrSignature};

use super::frost_stages::AwaitCommitments1;

dyn_clone::clone_trait_object!(CeremonyStage<Message = SigningData, Result = SchnorrSignature>);

/// Sending half of the channel that additionally maps signer_idx -> accountId
/// and wraps the binary data into the appropriate for singning type
#[derive(Clone)]
pub struct SigningP2PSender {
    ceremony_id: CeremonyId,
    sender: RawP2PSender,
}

impl SigningP2PSender {
    fn new(
        validator_map: Arc<ValidatorMaps>,
        sender: UnboundedSender<InnerEvent>,
        ceremony_id: CeremonyId,
    ) -> Self {
        SigningP2PSender {
            ceremony_id,
            sender: RawP2PSender::new(validator_map, sender),
        }
    }
}

impl P2PSender for SigningP2PSender {
    type Data = SigningData;

    fn send(&self, reciever_idx: usize, data: Self::Data) {
        let msg: MultisigMessage = SigningDataWrapped::new(data, self.ceremony_id).into();
        let data = bincode::serialize(&msg)
            .unwrap_or_else(|e| panic!("Could not serialise MultisigMessage: {:?}: {}", msg, e));
        self.sender.send(reciever_idx, data);
    }
}

/// State that becomes available once we receive a request to sign for this ceremony (it is possible
/// to start receiving ceremony messages before then)
#[derive(Clone)]
struct AuthorisedSigningState {
    ceremony_id: CeremonyId,
    /// State specific to the current ceremony stage
    stage: Option<Box<dyn CeremonyStage<Message = SigningData, Result = SchnorrSignature>>>,
    // TODO: this should be specialized to sending
    // results only (no p2p stuff)
    result_sender: EventSender,
    validator_map: Arc<ValidatorMaps>,
}

impl AuthorisedSigningState {
    fn send_result(&self, result: CeremonyOutcomeResult<SchnorrSignature>) {
        self.result_sender
            .send(InnerEvent::SigningResult(SigningOutcome {
                id: self.ceremony_id,
                result,
            }))
            .unwrap();
    }
}

/// State for a signing ceremony (potentially unauthorised, i.e. without a corresponding request to sign)
#[derive(Clone)]
pub struct SigningState {
    /// State for an authorised ceremony
    inner: Option<AuthorisedSigningState>,
    /// Time point at which the current ceremony is considered expired and gets aborted
    should_expire_at: std::time::Instant,
    /// Messages that arrived a bit early and should be
    /// processed once we transition to the next stage
    delayed_messages: Vec<(AccountId, SigningData)>,
    logger: slog::Logger,
}

const STAGE_DURATION: Duration = Duration::from_secs(15);

impl SigningState {
    /// Upgrade existing state to authorised (with a key) if it isn't already,
    /// and process any delayed messages
    pub fn on_request_to_sign(
        &mut self,
        // TODO: see if we can make states unaware of their own
        // ceremony ids (by delegating p2p messaging upstream)
        ceremony_id: CeremonyId,
        signer_idx: usize,
        signer_idxs: Vec<usize>,
        key_info: KeygenResultInfo,
        data: MessageHash,
        event_sender: EventSender,
    ) {
        if self.inner.is_some() {
            slog::warn!(
                self.logger,
                "Request to sign ignored: duplicate ceremony_id"
            );
            return;
        }

        // Use the updated logger once we know the ceremony id
        self.logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        let common = CeremonyCommon {
            ceremony_id,
            p2p_sender: SigningP2PSender::new(
                key_info.validator_map.clone(),
                event_sender.clone(),
                ceremony_id,
            ),
            own_idx: signer_idx,
            all_idxs: signer_idxs,
            logger: self.logger.clone(),
        };

        let processor = AwaitCommitments1::new(
            common.clone(),
            SigningStateCommonInfo {
                data,
                key: key_info.key.clone(),
            },
        );

        let mut state = BroadcastStage::new(processor, common);

        state.init();

        self.inner = Some(AuthorisedSigningState {
            ceremony_id,
            stage: Some(Box::new(state)),
            validator_map: key_info.validator_map,
            result_sender: event_sender,
        });

        // Unlike other state transitions, we don't take into account
        // any time left in the prior stage when receiving a request
        // to sign (we don't want other parties to be able to
        // control when our stages time out)
        self.should_expire_at = Instant::now() + STAGE_DURATION;

        self.process_delayed();
    }

    /// Create State w/o access to key info (and other data available
    /// after a request to sign) with the only purpose of being
    /// able to keep delayed messages in the same place
    pub fn new_unauthorised(logger: slog::Logger) -> Self {
        SigningState {
            inner: None,
            delayed_messages: Default::default(),
            should_expire_at: Instant::now() + STAGE_DURATION,
            logger,
        }
    }

    /// Try to process delayed messages
    fn process_delayed(&mut self) {
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

    /// Add a message to be processed later
    fn add_delayed(&mut self, id: AccountId, m: SigningData) {
        match &self.inner {
            Some(_) => {
                slog::debug!(self.logger, "Delaying message {} from party [{}]", m, id);
            }
            None => {
                slog::debug!(
                    self.logger,
                    "Delaying message {} from party [{}] (pre signing request)",
                    m,
                    id,
                );
            }
        }

        self.delayed_messages.push((id, m));
    }

    /// Process message `m` from party `id`
    pub fn process_message(&mut self, id: AccountId, m: SigningData) {
        match &mut self.inner {
            None => {
                self.add_delayed(id, m);
            }
            Some(authorised_state) => {
                let stage = authorised_state.stage.as_mut().expect(
                    "The value is only None for a brief period of time, when we swap states, below",
                );

                // TODO: check that the party is a signer for this ceremony

                // delay the data if we are not ready for it
                if stage.should_delay(&m) {
                    self.add_delayed(id, m);
                    return;
                }

                // Check that the sender is a participant in the ceremony
                let sender_idx = match authorised_state.validator_map.get_idx(&id) {
                    Some(idx) => idx,
                    None => {
                        slog::debug!(self.logger, "Sender {} is not a valid participant", id);
                        return;
                    }
                };

                // Delegate actual processing to the current specific stage
                match stage.process_message(sender_idx, m) {
                    // All messages for the stage have been collected, try to
                    // finalize and see we can transition to the next stage
                    ProcessMessageResult::CollectedAll => {
                        let state = authorised_state.stage.take().unwrap();

                        // This is the only point at which we can get the result (apart from the timeout)
                        match state.finalize() {
                            StageResult::NextStage(mut stage) => {
                                slog::debug!(self.logger, "Ceremony transitions to {}", &stage);

                                stage.init();

                                authorised_state.stage = Some(stage);

                                // NOTE: we don't care when the state transition
                                // actually happened as we don't want other parties
                                // to be able to influence when our stages time out
                                // (any remaining time carries over to the next stage)
                                self.should_expire_at += STAGE_DURATION;

                                self.process_delayed();

                                // TODO: Should delete this state
                            }
                            StageResult::Error(bad_validators) => {
                                let blamed_parties = bad_validators
                                    .iter()
                                    .map(|idx| {
                                        authorised_state
                                            .validator_map
                                            .get_id(*idx)
                                            .expect("Should have all ids here")
                                            .clone()
                                    })
                                    .collect();

                                slog::warn!(
                                    self.logger,
                                    "Signing ceremony failed, blaming parties: {:?} ({:?})",
                                    &bad_validators,
                                    blamed_parties,
                                );

                                authorised_state.send_result(Err((Error::Invalid, blamed_parties)));
                            }
                            StageResult::Done(signature) => {
                                authorised_state.send_result(Ok(signature));

                                slog::debug!(
                                    self.logger,
                                    "Signing ceremony reached the final stage!"
                                );
                            }
                        }
                    }
                    ProcessMessageResult::Ignored | ProcessMessageResult::Progress => {
                        // Nothing to do
                    }
                }
            }
        }
    }

    /// Check expiration time, and report responsible nodes if expired
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
                        "Signing ceremony expired before a request to sign, blaming parties: {:?}",
                        blamed_ids
                    );

                    Some(blamed_ids)
                }
                Some(authorised_state) => {
                    // blame slow parties
                    let bladed_idx = authorised_state.stage.as_ref().unwrap().awaited_parties();

                    let blamed_ids = bladed_idx
                        .iter()
                        .map(|idx| authorised_state.validator_map.get_id(*idx).unwrap().clone())
                        .collect();

                    slog::warn!(
                        self.logger,
                        "Signing ceremony expired, blaming parties: {:?}",
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

/// Data common for signing stages
#[derive(Clone)]
pub struct SigningStateCommonInfo {
    pub(super) data: MessageHash,
    pub(super) key: Arc<KeygenResult>,
}
