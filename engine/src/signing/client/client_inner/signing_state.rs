use std::sync::Arc;
use std::time::{Duration, Instant};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc;

use crate::p2p::AccountId;

use super::client_inner::Error;
use crate::signing::{MessageHash, SigningOutcome};

use super::client_inner::{CeremonyOutcomeResult, MultisigMessage};

use super::common::{
    broadcast::{BroadcastStage, MessageWrapper},
    CeremonyCommon, CeremonyStage, KeygenResult, P2PSender, ProcessMessageResult, StageResult,
};

use super::frost::{SigningData, SigningDataWrapped};
use super::utils::ValidatorMaps;
use super::{InnerEvent, KeygenResultInfo, SchnorrSignature};

use super::frost_stages::AwaitCommitments1;

type EventSender = mpsc::UnboundedSender<InnerEvent>;

dyn_clone::clone_trait_object!(CeremonyStage<Message = SigningData, Result = SchnorrSignature>);

// MAXIM: when both Keygen and Signing use ceremony_id,
// we could look into removing this abstraction
#[derive(Clone)]
pub struct SigningMessageWrapper {
    ceremony_id: CeremonyId,
}

impl SigningMessageWrapper {
    pub fn new(ceremony_id: CeremonyId) -> Self {
        SigningMessageWrapper { ceremony_id }
    }
}

impl MessageWrapper<SigningData> for SigningMessageWrapper {
    fn wrap_and_serialize(&self, data: &SigningData) -> Vec<u8> {
        let msg: MultisigMessage = SigningDataWrapped::new(data.clone(), self.ceremony_id).into();

        bincode::serialize(&msg).unwrap()
    }
}

#[derive(Clone)]
struct AuthorisedSigningState {
    ceremony_id: CeremonyId,
    state: Option<Box<dyn CeremonyStage<Message = SigningData, Result = SchnorrSignature>>>,
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

/// State for a signing ceremony
#[derive(Clone)]
pub struct SigningState {
    inner: Option<AuthorisedSigningState>,
    should_expire_at: std::time::Instant,
    delayed_messages_by_id: Vec<(AccountId, SigningData)>,
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
        logger: &slog::Logger,
    ) {
        let common = CeremonyCommon {
            ceremony_id,
            p2p_sender: P2PSender::new(key_info.validator_map.clone(), event_sender.clone()),
            own_idx: signer_idx,
            all_idxs: signer_idxs.clone(),
            logger: logger.clone(),
        };

        let signing_common = SigningStateCommonInfo {
            data,
            key: key_info.key.clone(),
            logger: logger.clone(),
        };

        let processor = AwaitCommitments1::new(common.clone(), signing_common);

        let mut state =
            BroadcastStage::new(processor, common, SigningMessageWrapper::new(ceremony_id));

        state.init();

        self.inner = Some(AuthorisedSigningState {
            ceremony_id,
            state: Some(Box::new(state)),
            validator_map: key_info.validator_map.clone(),
            result_sender: event_sender,
        });

        // Unlike other state transitions, we don't take into account
        // any time left in the prior stage when receiving a request
        // to sign (we don't want other parties to be able to
        // control when our stages time out)
        self.should_expire_at = Instant::now() + STAGE_DURATION;

        self.process_delayed();
    }

    /// Create State w/o access to key info with
    /// the only purpose of being able to keep delayed
    /// messages in the same place
    pub fn new_unauthorised(logger: slog::Logger) -> Self {
        SigningState {
            inner: None,
            delayed_messages_by_id: Default::default(),
            should_expire_at: Instant::now() + STAGE_DURATION,
            logger,
        }
    }

    fn process_delayed(&mut self) {
        let messages = std::mem::take(&mut self.delayed_messages_by_id);

        // We neven process delayed messages pre signing request
        let ceremony_id = self.inner.as_ref().unwrap().ceremony_id;

        for (id, m) in messages {
            slog::debug!(
                self.logger,
                "Processing delayed message {} from party [{}] [ceremony id: {}]",
                m,
                id,
                ceremony_id
            );
            self.process_message(id, m);
        }
    }

    fn add_delayed(&mut self, id: AccountId, m: SigningData) {
        match &self.inner {
            Some(authorised_state) => {
                slog::debug!(
                    self.logger,
                    "Delaying message {} from party [{}] [ceremony id: {}]",
                    m,
                    id,
                    authorised_state.ceremony_id
                );
            }
            None => {
                slog::debug!(
                    self.logger,
                    "Delaying message {} from party [{}] [pre signing request]",
                    m,
                    id,
                );
            }
        }

        self.delayed_messages_by_id.push((id, m));
    }

    pub fn process_message(&mut self, id: AccountId, m: SigningData) {
        match &mut self.inner {
            None => {
                self.add_delayed(id, m);
            }
            Some(authorised_state) => {
                // We know it is safe to unwrap because the value is None
                // for a brief period of time when we swap states below
                let state = authorised_state.state.as_mut().unwrap();

                // TODO: check that the party is a signer for this ceremony
                if state.should_delay(&m) {
                    self.add_delayed(id, m);
                    return;
                }

                // Check that the validator has access to key
                let sender_idx = match authorised_state.validator_map.get_idx(&id) {
                    Some(idx) => idx,
                    None => return,
                };

                match state.process_message(sender_idx, m) {
                    ProcessMessageResult::CollectedAll => {
                        let state = authorised_state.state.take().unwrap();

                        // This is the only point at which we can get the result (apart from the timeout)
                        match state.finalize() {
                            StageResult::NextStage(mut stage) => {
                                slog::debug!(
                                    self.logger,
                                    "Ceremony transitions to {} [ceremony id: {}]",
                                    &stage,
                                    authorised_state.ceremony_id
                                );

                                stage.init();

                                authorised_state.state = Some(stage);

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
                                        authorised_state.validator_map.get_id(*idx).unwrap().clone()
                                    })
                                    .collect();

                                slog::warn!(
                                    self.logger,
                                    "Signing ceremony failed, blaming parties: {:?} ({:?}), [ceremony id: {}]",
                                    &bad_validators,
                                    blamed_parties,
                                    authorised_state.ceremony_id
                                );

                                authorised_state.send_result(Err((Error::Invalid, blamed_parties)));
                            }
                            StageResult::Done(signature) => {
                                authorised_state.send_result(Ok(signature));

                                slog::debug!(
                                    self.logger,
                                    "Signing ceremony reached the final stage! [ceremony id: {}]",
                                    authorised_state.ceremony_id
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
            let blamed_parties = match &self.inner {
                None => {
                    // blame the parties that tried to initiate the ceremony
                    self.delayed_messages_by_id
                        .iter()
                        .map(|(id, _)| id.clone())
                        .collect()
                }
                Some(authorised_state) => {
                    // blame slow parties
                    let late_idxs = authorised_state.state.as_ref().unwrap().awaited_parties();

                    late_idxs
                        .iter()
                        .map(|idx| authorised_state.validator_map.get_id(*idx).unwrap().clone())
                        .collect()
                }
            };

            Some(blamed_parties)
        } else {
            None
        }
    }

    #[cfg(test)]
    pub fn get_stage(&self) -> Option<String> {
        self.inner
            .as_ref()
            .and_then(|s| s.state.as_ref().map(|s| s.to_string()))
    }

    #[cfg(test)]
    pub fn set_expiry_time(&mut self, expiry_time: std::time::Instant) {
        self.should_expire_at = expiry_time;
    }
}

/// Info useful for most signing states
#[derive(Clone)]
pub struct SigningStateCommonInfo {
    pub(super) data: MessageHash,
    pub(super) key: Arc<KeygenResult>,
    logger: slog::Logger,
}
