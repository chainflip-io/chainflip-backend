use std::{collections::HashMap, time::Instant};

use crate::{
    eth::utils::pubkey_to_eth_addr,
    logging::CEREMONY_ID_KEY,
    p2p::{AccountId, P2PMessage, P2PMessageCommand},
    signing::{
        client::{KeyId, MultisigInstruction, PendingSigningInfo},
        KeyDB,
    },
};

use serde::{Deserialize, Serialize};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    common::KeygenResultInfo, frost::SigningDataWrapped, key_store::KeyStore,
    keygen_data::KeygenData, keygen_manager::KeygenManager, signing_manager::SigningManager,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchnorrSignature {
    /// Scalar component
    pub s: [u8; 32],
    /// Point component (commitment)
    pub r: secp256k1::PublicKey,
}

impl From<SchnorrSignature> for pallet_cf_vaults::SchnorrSigTruncPubkey {
    fn from(cfe_sig: SchnorrSignature) -> Self {
        Self {
            s: cfe_sig.s,
            k_times_g_address: pubkey_to_eth_addr(cfe_sig.r),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum MultisigMessage {
    KeyGenMessage(KeyGenMessageWrapped),
    SigningMessage(SigningDataWrapped),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyGenMessageWrapped {
    pub ceremony_id: CeremonyId,
    pub data: KeygenData,
}

impl KeyGenMessageWrapped {
    pub fn new<M>(ceremony_id: CeremonyId, m: M) -> Self
    where
        M: Into<KeygenData>,
    {
        KeyGenMessageWrapped {
            ceremony_id,
            data: m.into(),
        }
    }
}

impl From<KeyGenMessageWrapped> for MultisigMessage {
    fn from(wrapped: KeyGenMessageWrapped) -> Self {
        MultisigMessage::KeyGenMessage(wrapped)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Error {
    Unauthorised,
    Timeout,
    Invalid,
}

pub type CeremonyOutcomeResult<Output> = Result<Output, (Error, Vec<AccountId>)>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CeremonyOutcome<Id, Output> {
    pub id: Id,
    pub result: CeremonyOutcomeResult<Output>,
}
impl<Id, Output> CeremonyOutcome<Id, Output> {
    pub fn success(id: Id, output: Output) -> Self {
        Self {
            id,
            result: Ok(output),
        }
    }
    pub fn unauthorised(id: Id, bad_validators: Vec<AccountId>) -> Self {
        Self {
            id,
            result: Err((Error::Unauthorised, bad_validators)),
        }
    }
    pub fn timeout(id: Id, bad_validators: Vec<AccountId>) -> Self {
        Self {
            id,
            result: Err((Error::Timeout, bad_validators)),
        }
    }
    pub fn invalid(id: Id, bad_validators: Vec<AccountId>) -> Self {
        Self {
            id,
            result: Err((Error::Invalid, bad_validators)),
        }
    }
}

/// The final result of a keygen ceremony
pub type KeygenOutcome = CeremonyOutcome<CeremonyId, secp256k1::PublicKey>;
/// The final result of a Signing ceremony
pub type SigningOutcome = CeremonyOutcome<CeremonyId, SchnorrSignature>;

#[derive(Debug, PartialEq)]
pub enum InnerEvent {
    P2PMessageCommand(P2PMessageCommand),
    SigningResult(SigningOutcome),
    KeygenResult(KeygenOutcome),
}

pub type EventSender = tokio::sync::mpsc::UnboundedSender<InnerEvent>;

impl From<P2PMessageCommand> for InnerEvent {
    fn from(m: P2PMessageCommand) -> Self {
        InnerEvent::P2PMessageCommand(m)
    }
}

#[derive(Clone)]
pub struct MultisigClient<S>
where
    S: KeyDB,
{
    my_account_id: AccountId,
    key_store: KeyStore<S>,
    keygen: KeygenManager,
    pub signing_manager: SigningManager,
    inner_event_sender: UnboundedSender<InnerEvent>,
    /// Requests awaiting a key
    pending_requests_to_sign: HashMap<KeyId, Vec<PendingSigningInfo>>,
    logger: slog::Logger,
}

impl<S> MultisigClient<S>
where
    S: KeyDB,
{
    pub fn new(
        my_account_id: AccountId,
        db: S,
        inner_event_sender: UnboundedSender<InnerEvent>,
        logger: &slog::Logger,
    ) -> Self {
        MultisigClient {
            my_account_id: my_account_id.clone(),
            key_store: KeyStore::new(db),
            keygen: KeygenManager::new(my_account_id.clone(), inner_event_sender.clone(), &logger),
            signing_manager: SigningManager::new(
                my_account_id,
                inner_event_sender.clone(),
                &logger,
            ),
            inner_event_sender,
            pending_requests_to_sign: Default::default(),
            logger: logger.clone(),
        }
    }

    /// Clean up expired states
    pub fn cleanup(&mut self) {
        self.keygen.cleanup();
        self.signing_manager.cleanup();

        // cleanup stale signing_info in pending_requests_to_sign
        let logger = &self.logger;
        self.pending_requests_to_sign
            .retain(|key_id, pending_signing_infos| {
                pending_signing_infos.retain(|pending| {
                    if pending.should_expire_at < Instant::now() {
                        slog::warn!(
                            logger,
                            "Request to sign expired waiting for key id: {:?}",
                            key_id;
                            CEREMONY_ID_KEY => pending.signing_info.ceremony_id,
                        );
                        return false;
                    }
                    true
                });
                !pending_signing_infos.is_empty()
            });
    }

    /// Process `instruction` issued internally (i.e. from SC or another local module)
    pub fn process_multisig_instruction(&mut self, instruction: MultisigInstruction) {
        match instruction {
            MultisigInstruction::KeyGen(keygen_info) => {
                // For now disable generating a new key when we already have one

                slog::debug!(
                    self.logger,
                    "Received a keygen request, participants: {:?}",
                    keygen_info.signers;
                    CEREMONY_ID_KEY => keygen_info.ceremony_id
                );

                self.keygen.on_keygen_request(keygen_info);
            }
            MultisigInstruction::Sign(sign_info) => {
                let key_id = &sign_info.key_id;

                slog::debug!(
                    self.logger,
                    "Received a request to sign, message_hash: {}, signers: {:?}",
                    sign_info.data, sign_info.signers;
                    CEREMONY_ID_KEY => sign_info.ceremony_id
                );
                match self.key_store.get_key(&key_id) {
                    Some(key) => {
                        self.signing_manager.on_request_to_sign(
                            sign_info.data,
                            key.clone(),
                            sign_info.signers,
                            sign_info.ceremony_id,
                        );
                    }
                    None => {
                        // The key is not ready, delay until either it is ready or timeout

                        slog::debug!(
                            self.logger,
                            "Delaying a request to sign for unknown key: {:?}",
                            sign_info.key_id;
                            CEREMONY_ID_KEY => sign_info.ceremony_id
                        );

                        self.pending_requests_to_sign
                            .entry(sign_info.key_id.clone())
                            .or_default()
                            .push(PendingSigningInfo::new(sign_info));
                    }
                }
            }
        }
    }

    /// Process requests to sign that required the key in `key_info`
    fn process_pending(&mut self, key_info: KeygenResultInfo) {
        if let Some(reqs) = self
            .pending_requests_to_sign
            .remove(&KeyId(key_info.key.get_public_key_bytes()))
        {
            for pending in reqs {
                let signing_info = pending.signing_info;
                slog::debug!(
                    self.logger,
                    "Processing a pending requests to sign";
                    CEREMONY_ID_KEY => signing_info.ceremony_id
                );

                self.signing_manager.on_request_to_sign(
                    signing_info.data,
                    key_info.clone(),
                    signing_info.signers,
                    signing_info.ceremony_id,
                )
            }
        }
    }

    fn on_key_generated(&mut self, ceremony_id: CeremonyId, key_info: KeygenResultInfo) {
        use crate::signing::crypto::ECPoint;

        self.key_store
            .set_key(KeyId(key_info.key.get_public_key_bytes()), key_info.clone());
        self.process_pending(key_info.clone());

        // NOTE: we only notify the SC after we have successfully saved the key

        if let Err(err) =
            self.inner_event_sender
                .send(InnerEvent::KeygenResult(KeygenOutcome::success(
                    ceremony_id,
                    key_info.key.get_public_key().get_element(),
                )))
        {
            // TODO: alert
            slog::error!(
                self.logger,
                "Could not sent KeygenOutcome::Success: {}",
                err
            );
        }
    }

    /// Process message from another validator
    pub fn process_p2p_message(&mut self, p2p_message: P2PMessage) {
        let P2PMessage { sender_id, data } = p2p_message;
        let multisig_message: Result<MultisigMessage, _> = bincode::deserialize(&data);

        match multisig_message {
            Ok(MultisigMessage::KeyGenMessage(keygen_message)) => {
                // NOTE: we should be able to process Keygen messages
                // even when we are "signing"... (for example, if we want to
                // generate a new key)

                let ceremony_id = keygen_message.ceremony_id;

                if let Some(key) = self.keygen.process_keygen_data(sender_id, keygen_message) {
                    self.on_key_generated(ceremony_id, key);
                    // NOTE: we could already delete the state here, but it is
                    // not necessary as it will be deleted by "cleanup"
                }
            }
            Ok(MultisigMessage::SigningMessage(signing_message)) => {
                // NOTE: we should be able to process Signing messages
                // even when we are generating a new key (for example,
                // we should be able to receive phase1 messages before we've
                // finalized the signing key locally)
                self.signing_manager
                    .process_signing_data(sender_id, signing_message);
            }
            Err(_) => {
                slog::warn!(
                    self.logger,
                    "Cannot parse multisig message from {}, discarding",
                    sender_id
                );
            }
        }
    }
}

#[cfg(test)]
impl<S> MultisigClient<S>
where
    S: KeyDB,
{
    pub fn get_keygen(&self) -> &KeygenManager {
        &self.keygen
    }

    pub fn get_key(&self, key_id: &KeyId) -> Option<&KeygenResultInfo> {
        self.key_store.get_key(key_id)
    }

    pub fn get_db(&self) -> &S {
        self.key_store.get_db()
    }

    pub fn get_my_account_id(&self) -> AccountId {
        self.my_account_id.clone()
    }

    /// Change the time we wait until deleting all unresolved states
    pub fn expire_all(&mut self) {
        self.keygen.expire_all();
        self.signing_manager.expire_all();

        self.pending_requests_to_sign.retain(|_, pending_infos| {
            for pending in pending_infos {
                pending.set_expiry_time(std::time::Instant::now());
            }
            true
        });
    }
}
