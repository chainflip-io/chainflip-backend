use anyhow::{bail, Context, Result};
use futures::FutureExt;
use futures::{stream::FuturesUnordered, StreamExt};
use std::collections::{hash_map, BTreeSet, HashMap};
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::constants::CEREMONY_ID_WINDOW;
use crate::multisig::client;
use crate::multisig::client::common::{KeygenFailureReason, SigningFailureReason};
use crate::multisig::client::keygen::generate_key_data_until_compatible;
use crate::multisig::client::CeremonyRequestDetails;
use crate::multisig::crypto::ECScalar;
use crate::multisig::crypto::{CryptoScheme, ECPoint, Rng};
use crate::multisig_p2p::OutgoingMultisigStageMessages;
use crate::task_scope::{with_task_scope, Scope, ScopedJoinHandle};
use cf_primitives::{AuthorityCount, CeremonyId};
use state_chain_runtime::AccountId;

use client::{
    ceremony_runner::CeremonyRunner, signing::frost::SigningData, utils::PartyIdxMapping,
};

use tokio::sync::oneshot;

use crate::logging::{CEREMONY_ID_KEY, CEREMONY_TYPE_KEY};

use client::common::{
    broadcast::BroadcastStage, CeremonyCommon, CeremonyFailureReason, KeygenResultInfo,
};

use crate::multisig::MessageHash;

use super::common::{CeremonyStage, PreProcessStageDataCheck};
use super::keygen::{HashCommitments1, HashContext, KeygenData};
use super::{CeremonyRequest, MultisigData, MultisigMessage};

pub type CeremonyOutcome<Ceremony> = Result<
    <Ceremony as CeremonyTrait>::Output,
    (
        BTreeSet<AccountId>,
        CeremonyFailureReason<<Ceremony as CeremonyTrait>::FailureReason>,
    ),
>;

pub type CeremonyResultSender<Ceremony> = oneshot::Sender<CeremonyOutcome<Ceremony>>;
pub type CeremonyResultReceiver<Ceremony> = oneshot::Receiver<CeremonyOutcome<Ceremony>>;

/// Ceremony trait combines type parameters that are often used together
pub trait CeremonyTrait: 'static {
    // Determines which curve and signing method to use
    type Crypto: CryptoScheme;
    // The type of data that will be used in p2p for this ceremony type
    type Data: Debug
        + Display
        + PreProcessStageDataCheck
        + TryFrom<
            MultisigData<<Self::Crypto as CryptoScheme>::Point>,
            Error = MultisigData<<Self::Crypto as CryptoScheme>::Point>,
        > + Send
        + 'static;
    type Request: Send + 'static;
    /// The product of a successful ceremony result
    type Output: Debug + Send + 'static;
    type FailureReason: Debug + Display + Send + 'static + PartialEq + Ord;
}

pub struct KeygenCeremony<C> {
    _phantom: PhantomData<C>,
}

impl<C: CryptoScheme> CeremonyTrait for KeygenCeremony<C> {
    type Crypto = C;
    type Data = KeygenData<<C as CryptoScheme>::Point>;
    type Request = CeremonyRequest<C>;
    type Output = KeygenResultInfo<<C as CryptoScheme>::Point>;
    type FailureReason = KeygenFailureReason;
}

pub struct SigningCeremony<C> {
    _phantom: PhantomData<C>,
}

impl<C: CryptoScheme> CeremonyTrait for SigningCeremony<C> {
    type Crypto = C;
    type Data = SigningData<<C as CryptoScheme>::Point>;
    type Request = CeremonyRequest<C>;
    type Output = <C as CryptoScheme>::Signature;
    type FailureReason = SigningFailureReason;
}

/// Responsible for mapping ceremonies to the corresponding states and
/// generating signer indexes based on the list of parties
pub struct CeremonyManager<C: CryptoScheme> {
    my_account_id: AccountId,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    signing_states: CeremonyStates<SigningCeremony<C>>,
    keygen_states: CeremonyStates<KeygenCeremony<C>>,
    allowing_high_pubkey: bool,
    latest_ceremony_id: CeremonyId,
    logger: slog::Logger,
}

// A CeremonyStage for either keygen or signing
pub type DynStage<Ceremony> = Box<
    dyn CeremonyStage<
            Message = <Ceremony as CeremonyTrait>::Data,
            Result = <Ceremony as CeremonyTrait>::Output,
            FailureReason = <Ceremony as CeremonyTrait>::FailureReason,
        > + Send
        + Sync,
>;

// A ceremony request that has passed initial checks and setup its initial stage
pub struct PreparedRequest<C: CeremonyTrait> {
    pub initial_stage: DynStage<C>,
}

// Initial checks and setup before sending the request to the `CeremonyRunner`
pub fn prepare_signing_request<C: CryptoScheme>(
    ceremony_id: CeremonyId,
    own_account_id: &AccountId,
    signers: BTreeSet<AccountId>,
    key_info: KeygenResultInfo<C::Point>,
    data: MessageHash,
    outgoing_p2p_message_sender: &UnboundedSender<OutgoingMultisigStageMessages>,
    rng: Rng,
    logger: &slog::Logger,
) -> Result<
    PreparedRequest<SigningCeremony<C>>,
    CeremonyFailureReason<<SigningCeremony<C> as CeremonyTrait>::FailureReason>,
> {
    // Check that we have enough signers
    let minimum_signers_needed = key_info.params.threshold + 1;
    let signers_len: AuthorityCount = signers.len().try_into().expect("too many signers");
    if signers_len < minimum_signers_needed {
        slog::debug!(
            logger,
            "Request to sign invalid: not enough signers ({}/{})",
            signers.len(),
            minimum_signers_needed
        );

        return Err(CeremonyFailureReason::Other(
            SigningFailureReason::NotEnoughSigners,
        ));
    }

    // Generate signer indexes
    let (own_idx, signer_idxs) =
        match map_ceremony_parties(own_account_id, &signers, &key_info.validator_mapping) {
            Ok(result) => result,
            Err(reason) => {
                slog::debug!(logger, "Request to sign invalid: {}", reason);
                return Err(CeremonyFailureReason::InvalidParticipants);
            }
        };

    // Prepare initial ceremony stage
    let initial_stage = {
        use super::signing::{frost_stages::AwaitCommitments1, SigningStateCommonInfo};

        let common = CeremonyCommon {
            ceremony_id,
            outgoing_p2p_message_sender: outgoing_p2p_message_sender.clone(),
            validator_mapping: key_info.validator_mapping,
            own_idx,
            all_idxs: signer_idxs,
            logger: logger.clone(),
            rng,
        };

        let processor = AwaitCommitments1::<C>::new(
            common.clone(),
            SigningStateCommonInfo {
                data,
                key: key_info.key,
            },
        );

        Box::new(BroadcastStage::new(processor, common))
    };

    Ok(PreparedRequest { initial_stage })
}

// Initial checks and setup before sending the request to the `CeremonyRunner`
pub fn prepare_keygen_request<C: CryptoScheme>(
    ceremony_id: CeremonyId,
    own_account_id: &AccountId,
    participants: BTreeSet<AccountId>,
    outgoing_p2p_message_sender: &UnboundedSender<OutgoingMultisigStageMessages>,
    rng: Rng,
    allowing_high_pubkey: bool,
    logger: &slog::Logger,
) -> Result<
    PreparedRequest<KeygenCeremony<C>>,
    CeremonyFailureReason<<KeygenCeremony<C> as CeremonyTrait>::FailureReason>,
> {
    let validator_mapping = Arc::new(PartyIdxMapping::from_participants(participants.clone()));

    let (our_idx, signer_idxs) =
        match map_ceremony_parties(own_account_id, &participants, &validator_mapping) {
            Ok(res) => res,
            Err(reason) => {
                slog::debug!(logger, "Keygen request invalid: {}", reason);

                return Err(CeremonyFailureReason::InvalidParticipants);
            }
        };

    let initial_stage = {
        let common = CeremonyCommon {
            ceremony_id,
            outgoing_p2p_message_sender: outgoing_p2p_message_sender.clone(),
            validator_mapping,
            own_idx: our_idx,
            all_idxs: signer_idxs,
            logger: logger.clone(),
            rng,
        };

        let processor = HashCommitments1::new(
            common.clone(),
            allowing_high_pubkey,
            generate_keygen_context(ceremony_id, participants),
        );

        Box::new(BroadcastStage::new(processor, common))
    };

    Ok(PreparedRequest { initial_stage })
}

fn map_ceremony_parties(
    own_account_id: &AccountId,
    participants: &BTreeSet<AccountId>,
    validator_mapping: &PartyIdxMapping,
) -> Result<(AuthorityCount, BTreeSet<AuthorityCount>), &'static str> {
    assert!(
        participants.contains(own_account_id),
        "we are not among participants"
    );

    // It should be impossible to fail here because of the check above,
    // but I don't like unwrapping (would be better if we
    // could combine this with the check above)
    let our_idx = validator_mapping
        .get_idx(own_account_id)
        .ok_or("could not derive our idx")?;

    // Check that signer ids are known for this key
    let signer_idxs = validator_mapping
        .get_all_idxs(participants)
        .map_err(|_| "invalid participants")?;

    Ok((our_idx, signer_idxs))
}

impl<C: CryptoScheme> CeremonyManager<C> {
    pub fn new(
        my_account_id: AccountId,
        outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
        latest_ceremony_id: CeremonyId,
        logger: &slog::Logger,
    ) -> Self {
        CeremonyManager {
            my_account_id,
            outgoing_p2p_message_sender,
            signing_states: CeremonyStates::new(),
            keygen_states: CeremonyStates::new(),
            allowing_high_pubkey: false,
            latest_ceremony_id,
            logger: logger.clone(),
        }
    }

    async fn on_request(
        &mut self,
        request: CeremonyRequest<C>,
        scope: &Scope<'_, anyhow::Result<()>, true>,
    ) {
        // Always update the latest ceremony id, even if we are not participating
        self.update_latest_ceremony_id(request.ceremony_id);

        match request.details {
            Some(CeremonyRequestDetails::Keygen(details)) => self.on_keygen_request(
                request.ceremony_id,
                details.participants,
                details.rng,
                details.result_sender,
                scope,
            ),
            Some(CeremonyRequestDetails::Sign(details)) => {
                self.on_request_to_sign(
                    request.ceremony_id,
                    details.participants,
                    details.data,
                    details.keygen_result_info,
                    details.rng,
                    details.result_sender,
                    scope,
                );
            }
            None => { /* Not participating in the ceremony, so do nothing */ }
        }
    }

    pub async fn run(
        mut self,
        mut ceremony_request_receiver: UnboundedReceiver<CeremonyRequest<C>>,
        mut incoming_p2p_message_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
    ) -> Result<()> {
        with_task_scope::<_,()>(|scope|
            async {
                loop {
                    tokio::select! {
                        Some(request) = ceremony_request_receiver.recv() => {
                            self.on_request(request, scope).await;
                        }
                        Some((sender_id, data)) = incoming_p2p_message_receiver.recv() => {

                            // At this point we know the messages to be for the
                            // appropriate curve (as defined by `C`)
                            match bincode::deserialize(&data) {
                                Ok(message) => self.process_p2p_message(sender_id, message, scope),
                                Err(_) => {
                                    slog::warn!(self.logger, "Failed to deserialize message from: {}", sender_id);
                                },
                            }
                        }
                        Some((id, outcome)) = self.signing_states.ceremony_futures.next() => {
                            self.signing_states.finalize_ceremony(id, outcome);
                        }
                        Some((id, outcome)) = self.keygen_states.ceremony_futures.next() => {
                            self.keygen_states.finalize_ceremony(id, outcome);
                        }
                    }
                }
            }.boxed()).await
    }

    /// Process a keygen request
    pub fn on_keygen_request(
        &mut self,
        ceremony_id: CeremonyId,
        participants: BTreeSet<AccountId>,
        rng: Rng,
        result_sender: CeremonyResultSender<KeygenCeremony<C>>,
        scope: &Scope<'_, anyhow::Result<()>, true>,
    ) {
        assert!(
            !participants.is_empty(),
            "Keygen request has no participants"
        );

        let logger_with_ceremony_id = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        slog::debug!(logger_with_ceremony_id, "Processing a keygen request");

        if participants.len() == 1 {
            let _result = result_sender.send(Ok(self.single_party_keygen(rng)));
            return;
        }

        let request = match prepare_keygen_request(
            ceremony_id,
            &self.my_account_id,
            participants,
            &self.outgoing_p2p_message_sender,
            rng,
            self.allowing_high_pubkey,
            &logger_with_ceremony_id,
        ) {
            Ok(request) => request,
            Err(failed_outcome) => {
                let _res = result_sender.send(CeremonyOutcome::<KeygenCeremony<C>>::Err((
                    BTreeSet::new(),
                    failed_outcome,
                )));

                // Remove a possible unauthorised ceremony
                self.keygen_states
                    .cleanup_unauthorised_ceremony(&ceremony_id);
                return;
            }
        };

        let ceremony_handle =
            self.keygen_states
                .get_state_or_create_unauthorized(ceremony_id, scope, &self.logger);

        ceremony_handle
            .on_request(request, result_sender)
            .with_context(|| format!("Invalid keygen request with ceremony id {}", ceremony_id))
            .unwrap();
    }

    /// Process a request to sign
    pub fn on_request_to_sign(
        &mut self,
        ceremony_id: CeremonyId,
        signers: BTreeSet<AccountId>,
        data: MessageHash,
        key_info: KeygenResultInfo<C::Point>,
        rng: Rng,
        result_sender: CeremonyResultSender<SigningCeremony<C>>,
        scope: &Scope<'_, anyhow::Result<()>, true>,
    ) {
        assert!(!signers.is_empty(), "Request to sign has no signers");

        let logger_with_ceremony_id = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        slog::debug!(logger_with_ceremony_id, "Processing a request to sign");

        if signers.len() == 1 {
            let _result = result_sender.send(Ok(self.single_party_signing(data, key_info, rng)));
            return;
        }

        let request = match prepare_signing_request(
            ceremony_id,
            &self.my_account_id,
            signers,
            key_info,
            data,
            &self.outgoing_p2p_message_sender,
            rng,
            &logger_with_ceremony_id,
        ) {
            Ok(request) => request,
            Err(failed_outcome) => {
                let _res = result_sender.send(CeremonyOutcome::<SigningCeremony<C>>::Err((
                    BTreeSet::new(),
                    failed_outcome,
                )));

                // Remove a possible unauthorised ceremony
                self.signing_states
                    .cleanup_unauthorised_ceremony(&ceremony_id);
                return;
            }
        };

        // We have the key and have received a request to sign
        let ceremony_handle =
            self.signing_states
                .get_state_or_create_unauthorized(ceremony_id, scope, &self.logger);

        ceremony_handle
            .on_request(request, result_sender)
            .with_context(|| format!("Invalid sign request with ceremony id {}", ceremony_id))
            .unwrap();
    }

    /// Process message from another validator
    pub fn process_p2p_message(
        &mut self,
        sender_id: AccountId,
        message: MultisigMessage<C::Point>,
        scope: &Scope<'_, anyhow::Result<()>, true>,
    ) {
        match message {
            MultisigMessage {
                ceremony_id,
                data: MultisigData::Keygen(data),
            } => self.keygen_states.process_data(
                sender_id,
                ceremony_id,
                data,
                self.latest_ceremony_id,
                scope,
                &self
                    .logger
                    .new(slog::o!(CEREMONY_ID_KEY => ceremony_id, CEREMONY_TYPE_KEY => "keygen")),
            ),
            MultisigMessage {
                ceremony_id,
                data: MultisigData::Signing(data),
            } => self.signing_states.process_data(
                sender_id,
                ceremony_id,
                data,
                self.latest_ceremony_id,
                scope,
                &self
                    .logger
                    .new(slog::o!(CEREMONY_ID_KEY => ceremony_id, CEREMONY_TYPE_KEY => "signing")),
            ),
        }
    }

    /// Override the latest ceremony id. Used to limit the spamming of unauthorised ceremonies.
    pub fn update_latest_ceremony_id(&mut self, ceremony_id: CeremonyId) {
        self.latest_ceremony_id = ceremony_id;
    }

    fn single_party_keygen(&self, rng: Rng) -> KeygenResultInfo<C::Point> {
        slog::info!(self.logger, "Performing solo keygen");

        let (_key_id, key_data) = generate_key_data_until_compatible(
            BTreeSet::from_iter([self.my_account_id.clone()]),
            30,
            rng,
        );
        key_data[&self.my_account_id].clone()
    }

    fn single_party_signing(
        &self,
        data: MessageHash,
        keygen_result_info: KeygenResultInfo<C::Point>,
        mut rng: Rng,
    ) -> C::Signature {
        slog::info!(self.logger, "Performing solo signing");

        let key = &keygen_result_info.key.key_share;

        let nonce = <C::Point as ECPoint>::Scalar::random(&mut rng);

        let r = C::Point::from_scalar(&nonce);

        let sigma = client::signing::frost::generate_schnorr_response::<C>(
            &key.x_i, key.y, r, nonce, &data.0,
        );

        C::build_signature(sigma, r)
    }
}

#[cfg(test)]
impl<C: CryptoScheme> CeremonyManager<C> {
    pub fn get_signing_states_len(&self) -> usize {
        self.signing_states.len()
    }

    pub fn get_keygen_states_len(&self) -> usize {
        self.keygen_states.len()
    }
}

/// Create unique deterministic context used for generating a ZKP to prevent replay attacks
pub fn generate_keygen_context(
    ceremony_id: CeremonyId,
    signers: BTreeSet<AccountId>,
) -> HashContext {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    hasher.update(ceremony_id.to_be_bytes());

    // NOTE: it should be sufficient to use ceremony_id as context as
    // we never reuse the same id for different ceremonies, but lets
    // put the signers in to make the context hard to predict as well
    for id in signers {
        hasher.update(id);
    }

    HashContext(*hasher.finalize().as_ref())
}

struct CeremonyStates<Ceremony: CeremonyTrait> {
    // Collection of all ceremony handles used to send data to the ceremony tasks
    ceremony_handles: HashMap<CeremonyId, CeremonyHandle<Ceremony>>,
    /// used to get notified when a ceremony is finished
    ceremony_futures: FuturesUnordered<ScopedJoinHandle<(CeremonyId, CeremonyOutcome<Ceremony>)>>,
}

impl<Ceremony: CeremonyTrait> CeremonyStates<Ceremony> {
    fn new() -> Self {
        Self {
            ceremony_handles: HashMap::new(),
            ceremony_futures: Default::default(),
        }
    }

    /// Process ceremony data arriving from a peer,
    fn process_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: Ceremony::Data,
        latest_ceremony_id: CeremonyId,
        scope: &Scope<'_, anyhow::Result<()>, true>,
        logger: &slog::Logger,
    ) {
        slog::debug!(logger, "Received data {}", &data);

        // Get the existing ceremony or create an unauthorised one (with ceremony id tracking check)
        let ceremony_handle = match self.ceremony_handles.entry(ceremony_id) {
            hash_map::Entry::Vacant(entry) => {
                // Only a ceremony id that is within the ceremony id window can create unauthorised ceremonies
                if ceremony_id > latest_ceremony_id
                    && ceremony_id <= latest_ceremony_id + CEREMONY_ID_WINDOW
                {
                    let (ceremony_handle, task_handle) =
                        CeremonyHandle::spawn(ceremony_id, scope, logger);
                    self.ceremony_futures.push(task_handle);
                    entry.insert(ceremony_handle)
                } else {
                    slog::debug!(
                        logger,
                        "Ignoring data: unexpected ceremony id {}",
                        ceremony_id
                    );
                    return;
                }
            }
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
        };

        ceremony_handle
            .message_sender
            .send((sender_id, data))
            .unwrap();
    }

    /// Returns the state for the given ceremony id if it exists,
    /// otherwise creates a new unauthorized one
    fn get_state_or_create_unauthorized(
        &mut self,
        ceremony_id: CeremonyId,
        scope: &Scope<'_, anyhow::Result<()>, true>,
        logger: &slog::Logger,
    ) -> &mut CeremonyHandle<Ceremony> {
        self.ceremony_handles.entry(ceremony_id).or_insert_with(|| {
            let (ceremony_handle, task_handle) = CeremonyHandle::spawn(ceremony_id, scope, logger);

            self.ceremony_futures.push(task_handle);

            ceremony_handle
        })
    }

    /// Send the outcome of the ceremony and remove its state
    fn finalize_ceremony(
        &mut self,
        ceremony_id: CeremonyId,
        ceremony_outcome: CeremonyOutcome<Ceremony>,
    ) {
        match self
            .ceremony_handles
            .remove(&ceremony_id)
            .expect("Should have handle")
            .request_state
        {
            CeremonyRequestState::Authorised(result_sender) => {
                let _result = result_sender.send(ceremony_outcome);
            }
            CeremonyRequestState::Unauthorised(_) => {
                // Only caused by `CeremonyFailureReason::ExpiredBeforeBeingAuthorized`,
                // We do not report timeout of unauthorised ceremonies
            }
        }
    }

    /// Removing any state associated with the unauthorized ceremony
    fn cleanup_unauthorised_ceremony(&mut self, ceremony_id: &CeremonyId) {
        if let Some(removed) = self.ceremony_handles.remove(ceremony_id) {
            assert!(
                matches!(removed.request_state, CeremonyRequestState::Unauthorised(_)),
                "Expected an unauthorised ceremony"
            );
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.ceremony_handles.len()
    }
}

// ==================

/// Contains the result sender and the channels used to send data to a running ceremony
struct CeremonyHandle<Ceremony: CeremonyTrait> {
    pub message_sender: UnboundedSender<(AccountId, Ceremony::Data)>,
    pub request_state: CeremonyRequestState<Ceremony>,
}

/// Contains either the request sender or the result sender depending on the state of the ceremony
enum CeremonyRequestState<Ceremony: CeremonyTrait> {
    /// Initial state before we have received the request from the SC.
    /// Contains the oneshot channel used to relay the request to the ceremony task.
    Unauthorised(oneshot::Sender<PreparedRequest<Ceremony>>),
    /// State after receiving the request from the SC.
    /// Contains the result sender that is used to send the ceremony outcome.
    Authorised(CeremonyResultSender<Ceremony>),
}

impl<Ceremony: CeremonyTrait> CeremonyHandle<Ceremony> {
    fn spawn(
        cid: CeremonyId,
        scope: &Scope<'_, anyhow::Result<()>, true>,
        logger: &slog::Logger,
    ) -> (
        Self,
        ScopedJoinHandle<(CeremonyId, CeremonyOutcome<Ceremony>)>,
    ) {
        let (msg_s, msg_r) = mpsc::unbounded_channel();
        let (req_s, req_r) = oneshot::channel();

        let task_handle = scope.spawn_with_handle(CeremonyRunner::<Ceremony>::run(
            cid,
            msg_r,
            req_r,
            logger.clone(),
        ));

        (
            CeremonyHandle {
                message_sender: msg_s,
                request_state: CeremonyRequestState::Unauthorised(req_s),
            },
            task_handle,
        )
    }

    fn on_request(
        &mut self,
        request: PreparedRequest<Ceremony>,
        result_sender: CeremonyResultSender<Ceremony>,
    ) -> Result<()> {
        // Transition to an authorized state by consuming the
        // request sender and storing the result sender
        if let CeremonyRequestState::Unauthorised(request_sender) = std::mem::replace(
            &mut self.request_state,
            CeremonyRequestState::Authorised(result_sender),
        ) {
            let _res = request_sender.send(request);
        } else {
            // Already in an authorised state, a request has already been sent to a ceremony with this id
            bail!("Duplicate ceremony id");
        }

        Ok(())
    }
}
