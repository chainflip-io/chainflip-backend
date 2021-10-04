use std::{collections::HashMap, convert::TryInto, fmt::Display, time::Duration};

use crate::{
    logging::COMPONENT_KEY,
    p2p::{AccountId, P2PMessage, P2PMessageCommand},
    signing::{
        client::{KeyId, MultisigInstruction, SigningInfo},
        crypto::{
            BigInt, ECPoint, ECScalar, KeyGenBroadcastMessage1, LocalSig, Signature, VerifiableSS,
            FE, GE,
        },
        db::KeyDB,
        MessageHash, MessageInfo,
    },
};

use pallet_cf_vaults::CeremonyId;
use slog::o;
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    common::KeygenResultInfo, key_store::KeyStore, keygen_manager::KeygenManager,
    signing_state_manager::SigningStateManager,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchnorrSignature {
    /// Scalar component
    // s: secp256k1::SecretKey,
    pub s: [u8; 32],
    /// Point component
    pub r: secp256k1::PublicKey,
}

impl From<Signature> for SchnorrSignature {
    fn from(sig: Signature) -> Self {
        let s: [u8; 32] = sig.sigma.get_element().as_ref().clone();
        let r = sig.v.get_element();
        SchnorrSignature { s, r }
    }
}

impl From<SchnorrSignature> for pallet_cf_vaults::SchnorrSigTruncPubkey {
    fn from(cfe_sig: SchnorrSignature) -> Self {
        // https://ethereum.stackexchange.com/questions/3542/how-are-ethereum-addresses-generated
        // Start with the public key (128 characters / 64 bytes)
        // Take the Keccak-256 hash of the public key. You should now have a string that is 64 characters / 32 bytes. (note: SHA3-256 eventually became the standard, but Ethereum uses Keccak)
        let hash = Keccak256::hash(&cfe_sig.r.serialize_uncompressed()).0;
        // Take the last 40 characters / 20 bytes of this public key (Keccak-256). Or, in other words, drop the first 24 characters / 12 bytes. These 40 characters / 20 bytes are the address. When prefixed with 0x it becomes 42 characters long.
        let eth_pub_key: [u8; 20] = hash[12..].try_into().expect("Is valid pubkey");
        Self {
            s: cfe_sig.s,
            eth_pub_key,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SigningData {
    Broadcast1(Broadcast1),
    Secret2(Secret2),
    LocalSig(LocalSig),
}

impl From<Broadcast1> for SigningData {
    fn from(bc1: Broadcast1) -> Self {
        SigningData::Broadcast1(bc1)
    }
}

impl Display for SigningData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // We are not interested in the exact contents most of the time
        match self {
            SigningData::Broadcast1(_) => write!(f, "Signing::Broadcast"),
            SigningData::Secret2(_) => write!(f, "Signing::Secret"),
            SigningData::LocalSig(_) => write!(f, "Signing::LocalSig"),
        }
    }
}

/// Protocol data plus the message to sign
#[derive(Serialize, Deserialize, Debug)]
pub struct SigningDataWrapped {
    pub data: SigningData,
    pub message: MessageInfo,
}

impl SigningDataWrapped {
    pub fn new<S>(data: S, message: MessageInfo) -> Self
    where
        S: Into<SigningData>,
    {
        SigningDataWrapped {
            data: data.into(),
            message,
        }
    }
}

impl From<SigningDataWrapped> for MultisigMessage {
    fn from(wrapped: SigningDataWrapped) -> Self {
        MultisigMessage::SigningMessage(wrapped)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum MultisigMessage {
    KeyGenMessage(KeyGenMessageWrapped),
    SigningMessage(SigningDataWrapped),
}

impl From<LocalSig> for SigningData {
    fn from(sig: LocalSig) -> Self {
        SigningData::LocalSig(sig)
    }
}

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Broadcast1 {
    pub bc1: KeyGenBroadcastMessage1,
    pub blind: BigInt,
    pub y_i: GE,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Secret2 {
    pub vss: VerifiableSS<GE>,
    pub secret_share: FE,
}

impl From<Secret2> for KeygenData {
    fn from(sec2: Secret2) -> Self {
        KeygenData::Secret2(sec2)
    }
}

impl From<Secret2> for SigningData {
    fn from(sec2: Secret2) -> Self {
        SigningData::Secret2(sec2)
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CeremonyOutcome<Id, Output> {
    pub id: Id,
    pub result: Result<Output, (Error, Vec<AccountId>)>,
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
pub type SigningOutcome = CeremonyOutcome<MessageInfo, SchnorrSignature>;

#[derive(Debug, PartialEq)]
pub enum InnerEvent {
    P2PMessageCommand(P2PMessageCommand),
    SigningResult(SigningOutcome),
    KeygenResult(KeygenOutcome),
}

#[derive(Clone)]
pub struct MultisigClientInner<S>
where
    S: KeyDB,
{
    my_account_id: AccountId,
    key_store: KeyStore<S>,
    keygen: KeygenManager,
    pub signing_manager: SigningStateManager,
    inner_event_sender: UnboundedSender<InnerEvent>,
    /// Requests awaiting a key
    pending_requests_to_sign: HashMap<KeyId, Vec<(MessageHash, SigningInfo)>>,
    logger: slog::Logger,
}

impl<S> MultisigClientInner<S>
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
        MultisigClientInner {
            my_account_id: my_account_id.clone(),
            key_store: KeyStore::new(db),
            keygen: KeygenManager::new(
                my_account_id.clone(),
                inner_event_sender.clone(),
                phase_timeout.clone(),
                logger,
            ),
            signing_manager: SigningStateManager::new(
                my_account_id,
                inner_event_sender.clone(),
                phase_timeout,
                logger,
            ),
            pending_requests_to_sign: Default::default(),
            inner_event_sender,
            logger: logger.new(o!(COMPONENT_KEY => "MultisigClientInner")),
        }
    }

    #[cfg(test)]
    pub fn get_keygen(&self) -> &KeygenManager {
        &self.keygen
    }

    #[cfg(test)]
    pub fn get_key(&self, key_id: KeyId) -> Option<&KeygenResultInfo> {
        self.key_store.get_key(key_id)
    }

    #[cfg(test)]
    pub fn get_db(&self) -> &S {
        self.key_store.get_db()
    }

    #[cfg(test)]
    pub fn get_my_account_id(&self) -> AccountId {
        self.my_account_id.clone()
    }

    /// Change the time we wait until deleting all unresolved states
    #[cfg(test)]
    pub fn set_timeout(&mut self, phase_timeout: Duration) {
        self.keygen.set_timeout(phase_timeout);
        self.signing_manager.set_timeout(phase_timeout);
    }

    /// Clean up expired states
    pub fn cleanup(&mut self) {
        self.keygen.cleanup();
        self.signing_manager.cleanup();
    }

    fn add_pending(&mut self, data: MessageHash, sign_info: SigningInfo) {
        slog::debug!(
            self.logger,
            "[{}] Delaying a request to sign: {}",
            self.my_account_id,
            data
        );

        // TODO: check for duplicates?

        let entry = self
            .pending_requests_to_sign
            .entry(sign_info.key_id.clone())
            .or_default();

        entry.push((data, sign_info));
    }

    /// Process `instruction` issued internally (i.e. from SC or another local module)
    pub fn process_multisig_instruction(&mut self, instruction: MultisigInstruction) {
        match instruction {
            MultisigInstruction::KeyGen(keygen_info) => {
                // For now disable generating a new key when we already have one

                slog::debug!(
                    self.logger,
                    "[{}] Received keygen instruction, ceremony_id: {}, participants: {:?}",
                    self.my_account_id,
                    keygen_info.ceremony_id,
                    keygen_info.signers
                );

                self.keygen.on_keygen_request(keygen_info);
            }
            MultisigInstruction::Sign(hash, sign_info) => {
                let key_id = sign_info.key_id.clone();
                slog::debug!(
                    self.logger,
                    "[{}] Received sign instruction for key_id: {}, message_hash: {}, signers: {:?}",
                    self.my_account_id,
                    hex::encode(&key_id.0),
                    hash,
                    &sign_info.signers
                );
                match self.key_store.get_key(key_id.clone()) {
                    Some(key) => {
                        self.signing_manager
                            .on_request_to_sign(hash, key.clone(), sign_info);
                    }
                    None => {
                        // the key is not ready, delay until it is
                        self.add_pending(hash, sign_info);
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
            slog::debug!(
                self.logger,
                "Processing pending requests to sign, count: {}",
                reqs.len()
            );
            for (data, info) in reqs {
                self.signing_manager
                    .on_request_to_sign(data, key_info.clone(), info)
            }
        }
    }

    fn on_key_generated(&mut self, ceremony_id: CeremonyId, key_info: KeygenResultInfo) {
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
        let multisig_message: Result<MultisigMessage, _> = serde_json::from_slice(&data);

        match multisig_message {
            Ok(MultisigMessage::KeyGenMessage(multisig_message)) => {
                // NOTE: we should be able to process Keygen messages
                // even when we are "signing"... (for example, if we want to
                // generate a new key)

                let ceremony_id = multisig_message.ceremony_id;

                if let Some(key) = self
                    .keygen
                    .process_keygen_message(sender_id, multisig_message)
                {
                    self.on_key_generated(ceremony_id, key);
                    // NOTE: we could already delete the state here, but it is
                    // not necessary as it will be deleted by "cleanup"
                }
            }
            Ok(MultisigMessage::SigningMessage(multisig_message)) => {
                // NOTE: we should be able to process Signing messages
                // even when we are generating a new key (for example,
                // we should be able to receive phase1 messages before we've
                // finalized the signing key locally)
                self.signing_manager
                    .process_signing_data(sender_id, multisig_message);
            }
            Err(_) => {
                slog::warn!(self.logger, "Cannot parse multisig message, discarding");
            }
        }
    }
}
