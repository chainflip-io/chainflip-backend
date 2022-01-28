#[macro_use]
mod utils;
mod ceremony_id_tracker;
mod common;
mod key_store;
pub mod keygen;
pub mod signing;
mod state_runner;

#[cfg(test)]
mod tests;

mod ceremony_manager;

#[cfg(test)]
mod genesis;

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::{
    common::{format_iterator, make_periodic_stream},
    eth::utils::pubkey_to_eth_addr,
    logging::{CEREMONY_ID_KEY, COMPONENT_KEY, REQUEST_TO_SIGN_EXPIRED},
    multisig::{
        crypto::{self, Rng},
        KeyDB, KeyId, MultisigInstruction,
    },
    multisig_p2p::OutgoingMultisigStageMessages,
};

use futures::{Future, StreamExt};
use slog::o;
use state_chain_runtime::AccountId;

use serde::{Deserialize, Serialize};

use pallet_cf_vaults::CeremonyId;

use key_store::KeyStore;

use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    oneshot,
};
use utilities::threshold_from_share_count;

use keygen::KeygenData;

pub use common::KeygenResultInfo;

#[cfg(test)]
pub use utils::ensure_unsorted;

use self::{
    ceremony_manager::CeremonyManager,
    signing::{frost::SigningData, PendingSigningInfo},
};

pub use keygen::KeygenOptions;

use super::{KeygenInfo, MessageHash, SigningInfo};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchnorrSignature {
    /// Scalar component
    pub s: [u8; 32],
    /// Point component (commitment)
    pub r: secp256k1::PublicKey,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThresholdParameters {
    /// Total number of key shares (equals the total number of parties in keygen)
    pub share_count: usize,
    /// Max number of parties that can *NOT* generate signature
    pub threshold: usize,
}

impl ThresholdParameters {
    pub fn from_share_count(share_count: usize) -> Self {
        ThresholdParameters {
            share_count,
            threshold: threshold_from_share_count(share_count as u32) as usize,
        }
    }
}

impl From<SchnorrSignature> for cf_chains::eth::SchnorrVerificationComponents {
    fn from(cfe_sig: SchnorrSignature) -> Self {
        Self {
            s: cfe_sig.s,
            k_times_g_addr: pubkey_to_eth_addr(cfe_sig.r),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MultisigData {
    Keygen(KeygenData),
    Signing(SigningData),
}

derive_try_from_variant!(KeygenData, MultisigData::Keygen, MultisigData);
derive_try_from_variant!(SigningData, MultisigData::Signing, MultisigData);

impl From<SigningData> for MultisigData {
    fn from(data: SigningData) -> Self {
        MultisigData::Signing(data)
    }
}

impl From<KeygenData> for MultisigData {
    fn from(data: KeygenData) -> Self {
        MultisigData::Keygen(data)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultisigMessage {
    ceremony_id: CeremonyId,
    data: MultisigData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CeremonyAbortReason {
    // Isn't used, but will once we re-enable unauthorised reporting this will be used again
    Unauthorised,
    Timeout,
    Invalid,
}

/// (Abort reason, blamed ceremony ids)
pub type CeremonyError = (CeremonyAbortReason, Vec<AccountId>);
pub type CeremonyOutcomeResult<Output> = Result<Output, CeremonyError>;

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
            result: Err((CeremonyAbortReason::Unauthorised, bad_validators)),
        }
    }
    pub fn timeout(id: Id, bad_validators: Vec<AccountId>) -> Self {
        Self {
            id,
            result: Err((CeremonyAbortReason::Timeout, bad_validators)),
        }
    }
    pub fn invalid(id: Id, bad_validators: Vec<AccountId>) -> Self {
        Self {
            id,
            result: Err((CeremonyAbortReason::Invalid, bad_validators)),
        }
    }
}

/// The final result of a keygen ceremony
pub type KeygenOutcome = CeremonyOutcome<CeremonyId, secp256k1::PublicKey>;
/// The final result of a Signing ceremony
pub type SigningOutcome = CeremonyOutcome<CeremonyId, SchnorrSignature>;

pub type MultisigOutcomeSender = tokio::sync::mpsc::UnboundedSender<MultisigOutcome>;

#[derive(Debug, Serialize, Deserialize)]
pub enum MultisigOutcome {
    Signing(SigningOutcome),
    Keygen(KeygenOutcome),
}

derive_try_from_variant!(SigningOutcome, MultisigOutcome::Signing, MultisigOutcome);
derive_try_from_variant!(KeygenOutcome, MultisigOutcome::Keygen, MultisigOutcome);

/// Multisig client is is responsible for persistently storing generated keys and
/// delaying signing requests (delegating the actual ceremony management to sub components)
pub struct MultisigClient<S>
where
    S: KeyDB,
{
    key_store: KeyStore<S>,
    multisig_instruction_sender: UnboundedSender<MultisigInstruction>,
    /// Requests awaiting a key
    pending_requests_to_sign: HashMap<KeyId, Vec<PendingSigningInfo>>,
    keygen_options: KeygenOptions,
    logger: slog::Logger,
}

impl<S> MultisigClient<S>
where
    S: KeyDB,
{
    pub fn inner_new(
        my_account_id: AccountId,
        db: S,
        multisig_instruction_sender: UnboundedSender<MultisigInstruction>,
        outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
        keygen_options: KeygenOptions,
        logger: &slog::Logger,
    ) -> (Self, CeremonyManager) {
        let logger = logger.new(o!(COMPONENT_KEY => "MultisigClient"));

        (
            MultisigClient {
                key_store: KeyStore::new(db),
                multisig_instruction_sender,
                pending_requests_to_sign: Default::default(),
                keygen_options,
                logger: logger.clone(),
            },
            CeremonyManager::new(my_account_id, outgoing_p2p_message_sender.clone(), &logger),
        )
    }

    pub fn new(
        my_account_id: AccountId,
        db: S,
        mut incoming_p2p_message_receiver: UnboundedReceiver<(AccountId, MultisigMessage)>,
        outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
        keygen_options: KeygenOptions,
        logger: &slog::Logger,
    ) -> (Self, impl Future) {
        let (multisig_instruction_sender, mut multisig_instruction_receiver) =
            tokio::sync::mpsc::unbounded_channel();

        let (multisig_client, mut ceremony_manager) = Self::inner_new(
            my_account_id,
            db,
            multisig_instruction_sender,
            outgoing_p2p_message_sender,
            keygen_options,
            logger,
        );

        (multisig_client, async move {
            // Stream outputs () approximately every ten seconds
            let mut cleanup_stream = make_periodic_stream(Duration::from_secs(10));

            use rand_legacy::FromEntropy;
            let mut rng = crypto::Rng::from_entropy();

            loop {
                tokio::select! {
                    Some((sender_id, message)) = incoming_p2p_message_receiver.recv() => {
                        ceremony_manager.process_p2p_message(sender_id, message);
                    }
                    Some(msg) = multisig_instruction_receiver.recv() => {
                        ceremony_manager.process_multisig_instruction(msg, &mut rng);
                    }
                    Some(()) = cleanup_stream.next() => {
                        ceremony_manager.cleanup();
                    }
                }
            }
        })
    }

    /// Process requests to sign that required the key in `key_info`
    fn process_pending_requests_to_sign(&mut self, keygen_result_info: KeygenResultInfo) {
        if let Some(reqs) = self
            .pending_requests_to_sign
            .remove(&KeyId(keygen_result_info.key.get_public_key_bytes()))
        {
            for pending in reqs {
                let signing_info = pending.signing_info;
                slog::debug!(
                    self.logger,
                    "Processing a pending requests to sign";
                    CEREMONY_ID_KEY => signing_info.ceremony_id
                );

                self.multisig_instruction_sender
                    .send(MultisigInstruction::Sign((
                        signing_info,
                        keygen_result_info.clone(),
                    )))
                    .unwrap();
            }
        }
    }

    /// Clean up expired states
    pub fn cleanup(&mut self) {
        //slog::trace!(self.logger, "Cleaning up multisig states");

        // cleanup stale signing_info in pending_requests_to_sign

        let now = Instant::now();
        let logger = &self.logger;

        self.pending_requests_to_sign
            .retain(|key_id, pending_signing_infos| {
                // TODO: Use drain_filter once stablized
                let (expired, pending) = std::mem::take(pending_signing_infos)
                    .into_iter()
                    .partition(|pending_signing_info| pending_signing_info.should_expire_at < now);
                *pending_signing_infos = pending;

                for expired_pending_signing_info in expired {
                    slog::warn!(
                        logger,
                        #REQUEST_TO_SIGN_EXPIRED,
                        "Request to sign expired waiting for key id: {:?}",
                        key_id;
                        CEREMONY_ID_KEY => expired_pending_signing_info.signing_info.ceremony_id,
                    );

                    expired_pending_signing_info
                        .signing_info
                        .result_sender
                        .send(Err((CeremonyAbortReason::Timeout, vec![])))
                        .unwrap();
                }

                !pending_signing_infos.is_empty()
            });
    }

    pub async fn keygen(
        &mut self,
        ceremony_id: CeremonyId,
        signers: Vec<AccountId>,
    ) -> Option<Result<secp256k1::PublicKey, CeremonyError>> {
        let (result_sender, result_receiver) = oneshot::channel();

        let keygen_info = KeygenInfo {
            ceremony_id,
            signers,
            result_sender,
        };

        self.multisig_instruction_sender
            .send(MultisigInstruction::Keygen((
                keygen_info,
                self.keygen_options.clone(),
            )))
            .unwrap();

        match result_receiver.await {
            Ok(result) => Some(result.map(|key_info| {
                use crate::multisig::crypto::ECPoint;

                // Wrap these in a mutex, lock here
                self.key_store
                    .set_key(KeyId(key_info.key.get_public_key_bytes()), key_info.clone());
                self.process_pending_requests_to_sign(key_info.clone());

                key_info.key.get_public_key().get_element()
            })),
            Err(_) => None,
        }
    }

    pub async fn sign(
        &mut self,
        ceremony_id: CeremonyId,
        signers: Vec<AccountId>,
        key_id: KeyId,
        data: MessageHash,
    ) -> Option<Result<SchnorrSignature, (CeremonyAbortReason, Vec<AccountId>)>> {
        let (result_sender, result_receiver) = oneshot::channel();

        let signing_info = SigningInfo {
            data,
            ceremony_id,
            key_id: key_id.clone(),
            signers,
            result_sender,
        };

        // Wrap in a mutex, lock here (See above)
        if let Some(keygen_result_info) = self.key_store.get_key(&key_id) {
            self.multisig_instruction_sender
                .send(MultisigInstruction::Sign((
                    signing_info,
                    keygen_result_info.clone(),
                )))
                .unwrap();
        } else {
            self.pending_requests_to_sign
                .entry(key_id)
                .or_default()
                .push(PendingSigningInfo::new(signing_info));
        }

        result_receiver.await.ok()
    }
}

#[cfg(test)]
impl<S> MultisigClient<S>
where
    S: KeyDB,
{
    pub fn get_key(&self, key_id: &KeyId) -> Option<&KeygenResultInfo> {
        self.key_store.get_key(key_id)
    }

    pub fn get_db(&self) -> &S {
        self.key_store.get_db()
    }

    pub fn force_stage_timeout(&mut self) {
        self.ceremony_manager.expire_all();

        self.pending_requests_to_sign.retain(|_, pending_infos| {
            for pending in pending_infos {
                pending.set_expiry_time(std::time::Instant::now());
            }
            true
        });

        self.cleanup();
    }
}
