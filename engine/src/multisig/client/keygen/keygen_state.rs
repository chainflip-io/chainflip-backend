use std::sync::Arc;
use std::time::{Duration, Instant};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;

use crate::logging::CEREMONY_ID_KEY;
use crate::p2p::AccountId;

use keygen::keygen_data::KeygenData;
use keygen::keygen_stages::AwaitCommitments1;

use crate::multisig::client;
use crate::multisig::client::keygen;

use client::{
    EventSender, InnerEvent, KeygenDataWrapped, KeygenResultInfo, MultisigMessage,
    ThresholdParameters,
};

use client::common::{
    broadcast::BroadcastStage, CeremonyCommon, CeremonyStage, KeygenResult, P2PSender,
    ProcessMessageResult, RawP2PSender, StageResult,
};
use client::utils::PartyIdxMapping;

/// Ceremony is authorised once we receive a keygen request with a corresponding ceremony_id
/// from our SC node, at which point the data in this struct also becomes available.
#[derive(Clone)]
struct KeygenStateAuthorised {
    ceremony_id: CeremonyId,
    /// State specific to the current ceremony stage
    stage: Option<Box<dyn CeremonyStage<Message = KeygenData, Result = KeygenResult>>>,
    // TODO: this should be specialized to sending
    // results only (no p2p stuff)
    result_sender: EventSender,
    validator_map: Arc<PartyIdxMapping>,
}

dyn_clone::clone_trait_object!(CeremonyStage<Message = KeygenData, Result = KeygenResult>);

#[derive(Clone)]
pub struct KeygenState {
    inner: Option<KeygenStateAuthorised>,
    logger: slog::Logger,
    delayed_messages: Vec<(AccountId, KeygenData)>,
    /// Time point at which the current ceremony is considered expired and gets aborted
    should_expire_at: std::time::Instant,
}

const MAX_STAGE_DURATION: Duration = Duration::from_secs(15);

impl KeygenState {
    pub fn new_unauthorised(logger: slog::Logger) -> Self {
        KeygenState {
            inner: None,
            logger,
            delayed_messages: Default::default(),
            should_expire_at: Instant::now() + MAX_STAGE_DURATION,
        }
    }

    pub fn on_keygen_request(
        &mut self,
        ceremony_id: CeremonyId,
        event_sender: EventSender,
        validator_map: Arc<PartyIdxMapping>,
        own_idx: usize,
        all_idxs: Vec<usize>,
    ) {
        self.logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        let common = CeremonyCommon {
            ceremony_id,
            // TODO: do not clone validator map
            p2p_sender: KeygenP2PSender::new(
                validator_map.clone(),
                event_sender.clone(),
                ceremony_id,
            ),
            own_idx,
            all_idxs,
            logger: self.logger.clone(),
        };

        let processor = AwaitCommitments1::new(ceremony_id, common.clone());

        let mut stage = BroadcastStage::new(processor, common);

        stage.init();

        self.inner = Some(KeygenStateAuthorised {
            stage: Some(Box::new(stage)),
            ceremony_id,
            validator_map,
            result_sender: event_sender,
        });

        // Unlike other state transitions, we don't take into account
        // any time left in the prior stage when receiving a request
        // to sign (we don't want other parties to be able to
        // control when our stages time out)
        self.should_expire_at = Instant::now() + MAX_STAGE_DURATION;

        self.process_delayed();
    }

    pub fn process_message(
        &mut self,
        sender_id: AccountId,
        data: KeygenData,
    ) -> Option<KeygenResultInfo> {
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
                let sender_idx = match authorised_state.validator_map.get_idx(&sender_id) {
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

                match stage.process_message(sender_idx, data) {
                    ProcessMessageResult::CollectedAll => {
                        let state = authorised_state.stage.take().unwrap();

                        match state.finalize() {
                            StageResult::NextStage(mut next_stage) => {
                                slog::debug!(
                                    self.logger,
                                    "Ceremony transitions to {}",
                                    &next_stage
                                );

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
                            StageResult::Error(_) => {
                                // TODO: should delete this state
                            }
                            StageResult::Done(keygen_result) => {
                                slog::debug!(
                                    self.logger,
                                    "Keygen ceremony reached the final stage!"
                                );

                                let params = ThresholdParameters::from_share_count(
                                    keygen_result.party_public_keys.len(),
                                );

                                let keygen_result_info = KeygenResultInfo {
                                    key: Arc::new(keygen_result),
                                    validator_map: authorised_state.validator_map.clone(),
                                    params,
                                };

                                return Some(keygen_result_info);
                            }
                        }
                    }
                    ProcessMessageResult::Ignored | ProcessMessageResult::Progress => {
                        // Nothing to do
                    }
                }
            }
        }

        None
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

    fn add_delayed(&mut self, id: AccountId, m: KeygenData) {
        match &self.inner {
            Some(_) => {
                slog::debug!(self.logger, "Delaying message {} from party [{}]", m, id);
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
                        "Keygen ceremony expired before a request to sign, blaming parties: {:?}",
                        blamed_ids
                    );

                    Some(blamed_ids)
                }
                Some(authorised_state) => {
                    // blame slow parties
                    let blamed_idx = authorised_state
                        .stage
                        .as_ref()
                        .expect("stage in authorised state is always present")
                        .awaited_parties();

                    let blamed_ids = blamed_idx
                        .iter()
                        .map(|idx| {
                            authorised_state
                                .validator_map
                                .get_id(*idx)
                                .expect("id for a blamed party should always be known")
                                .clone()
                        })
                        .collect();

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
}

#[cfg(test)]
impl KeygenState {
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

/// Sending half of the channel that additionally maps signer_idx -> accountId
/// and wraps the binary data into the appropriate for keygen type
#[derive(Clone)]
pub struct KeygenP2PSender {
    ceremony_id: CeremonyId,
    sender: RawP2PSender,
}

impl KeygenP2PSender {
    pub fn new(
        validator_map: Arc<PartyIdxMapping>,
        sender: UnboundedSender<InnerEvent>,
        ceremony_id: CeremonyId,
    ) -> Self {
        KeygenP2PSender {
            ceremony_id,
            sender: RawP2PSender::new(validator_map, sender),
        }
    }
}

impl P2PSender for KeygenP2PSender {
    type Data = KeygenData;

    fn send(&self, reciever_idx: usize, data: Self::Data) {
        let msg: MultisigMessage = KeygenDataWrapped::new(self.ceremony_id, data).into();
        let data = bincode::serialize(&msg).unwrap();
        self.sender.send(reciever_idx, data);
    }
}
