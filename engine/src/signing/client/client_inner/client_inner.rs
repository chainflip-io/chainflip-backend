use std::{collections::HashMap, fmt::Display, time::Duration};

use crate::{
    eth::utils::pubkey_to_eth_addr,
    p2p::{AccountId, P2PMessage, P2PMessageCommand},
    signing::{
        client::{KeyId, MultisigInstruction, SigningInfo},
        crypto::{BigInt, KeyGenBroadcastMessage1, Point, Scalar, VerifiableSS},
        KeyDB,
    },
};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    common::KeygenResultInfo, frost::SigningDataWrapped, key_store::KeyStore,
    keygen_manager::KeygenManager, signing_manager::SigningManager,
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

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Broadcast1 {
    pub bc1: KeyGenBroadcastMessage1,
    pub blind: BigInt,
    pub y_i: Point,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Secret2 {
    pub vss: VerifiableSS<Point>,
    pub secret_share: Scalar,
}

impl From<Secret2> for KeygenData {
    fn from(sec2: Secret2) -> Self {
        KeygenData::Secret2(sec2)
    }
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyGenMessageWrapped {
    pub ceremony_id: CeremonyId,
    pub message: KeygenData,
}

impl KeyGenMessageWrapped {
    pub fn new<M>(ceremony_id: CeremonyId, m: M) -> Self
    where
        M: Into<KeygenData>,
    {
        KeyGenMessageWrapped {
            ceremony_id,
            message: m.into(),
        }
    }
}

impl From<KeyGenMessageWrapped> for MultisigMessage {
    fn from(wrapped: KeyGenMessageWrapped) -> Self {
        MultisigMessage::KeyGenMessage(wrapped)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KeygenData {
    Broadcast1(Broadcast1),
    Secret2(Secret2),
}

impl From<Broadcast1> for KeygenData {
    fn from(bc1: Broadcast1) -> Self {
        KeygenData::Broadcast1(bc1)
    }
}

impl Display for KeygenData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            KeygenData::Broadcast1(_) => write!(f, "KeygenData::Broadcast1"),
            KeygenData::Secret2(_) => write!(f, "KeygenData::Secret2"),
        }
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
    // -> keygen_manager
    keygen: KeygenManager,
    pub signing_manager: SigningManager,
    inner_event_sender: UnboundedSender<InnerEvent>,
    /// Requests awaiting a key
    pending_requests_to_sign: HashMap<KeyId, Vec<SigningInfo>>,
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
        phase_timeout: Duration,
        logger: &slog::Logger,
    ) -> Self {
        MultisigClient {
            my_account_id: my_account_id.clone(),
            key_store: KeyStore::new(db),
            keygen: KeygenManager::new(
                my_account_id.clone(),
                inner_event_sender.clone(),
                phase_timeout,
                &logger,
            ),
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

        // TODO: cleanup stale signing_info in pending_requests_to_sign
    }

    /// Process `instruction` issued internally (i.e. from SC or another local module)
    pub fn process_multisig_instruction(&mut self, instruction: MultisigInstruction) {
        match instruction {
            MultisigInstruction::KeyGen(keygen_info) => {
                // either delete the below comment or link to an issue. It's confusing
                // For now disable generating a new key when we already have one

                slog::debug!(
                    self.logger,
                    "Received a keygen request, ceremony_id: {}, participants: {:?}",
                    keygen_info.ceremony_id,
                    keygen_info.signers
                );

                self.keygen.on_keygen_request(keygen_info);
            }
            MultisigInstruction::Sign(sign_info) => {
                let key_id = &sign_info.key_id;

                slog::debug!(
                    self.logger,
                    "Received a request to sign, ceremony_id: {}, message_hash: {}, signers: {:?}",
                    sign_info.ceremony_id,
                    sign_info.data,
                    sign_info.signers
                );
                match self.key_store.get_key(&key_id) {
                    Some(key) => {
                        self.signing_manager.start_signing_data(
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
                            "Delaying a request to sign for unknown key: {:?} [ceremony_id: {}]",
                            sign_info.key_id,
                            sign_info.ceremony_id
                        );

                        self.pending_requests_to_sign
                            .entry(sign_info.key_id.clone())
                            .or_default()
                            .push(sign_info);
                    }
                }
            }
        }
    }

    /// Process requests to sign that required the key in `key_info`
    /// This is triggered immediately after the key is generated
    fn process_signing_requests_pending_key_generation(&mut self, key_info: KeygenResultInfo) {
        if let Some(reqs) = self
            .pending_requests_to_sign
            .remove(&KeyId(key_info.key.get_public_key_bytes()))
        {
            for signing_info in reqs {
                slog::debug!(
                    self.logger,
                    "Processing pending request to sign [ceremony_id: {}]",
                    signing_info.ceremony_id
                );

                self.signing_manager.start_signing_data(
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
        self.process_signing_requests_pending_key_generation(key_info.clone());

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

    /// Process a message from another validator
    pub fn process_p2p_message(&mut self, p2p_message: P2PMessage) {
        let P2PMessage { sender_id, data } = p2p_message;
        let multisig_message: Result<MultisigMessage, _> = bincode::deserialize(&data);

        match multisig_message {
            Ok(MultisigMessage::KeyGenMessage(multisig_message)) => {
                let ceremony_id = multisig_message.ceremony_id;

                if let Some(key) = self
                    .keygen
                    .process_keygen_message(sender_id, multisig_message)
                {
                    self.on_key_generated(ceremony_id, key);
                    // even with this being true, why not just delete the state here if we can?
                    // letting it fall through is a bit harder to follow
                    // NOTE: we could already delete the state here, but it is
                    // not necessary as it will be deleted by "cleanup"
                }
            }
            Ok(MultisigMessage::SigningMessage(multisig_message)) => {
                // should this be "process signing request"
                // and the other one be "process signing data"? - seems like these method names
                // are the wrong way around
                self.signing_manager
                    .process_signing_data(sender_id, multisig_message);
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
    }
}
