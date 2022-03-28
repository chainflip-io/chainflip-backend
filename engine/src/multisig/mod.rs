//! Multisig signing and keygen

/// Multisig client
pub mod client;
/// Provides cryptographic primitives used by the multisig client
mod crypto;
/// Storage for the keys
pub mod db;

#[cfg(test)]
mod tests;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use serde::{Deserialize, Serialize};

use std::time::Duration;

use crate::{common, logging::COMPONENT_KEY, multisig_p2p::OutgoingMultisigStageMessages};
use slog::o;
use state_chain_runtime::AccountId;

pub use client::{
    KeygenOptions, KeygenOutcome, MultisigClient, MultisigMessage, MultisigOutcome,
    SchnorrSignature, SigningOutcome,
};

pub use db::{KeyDB, PersistentKeyDB};

#[cfg(test)]
pub use db::KeyDBMock;

pub use self::client::{keygen::KeygenRequest, signing::SigningRequest};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct MessageHash(pub [u8; 32]);

impl std::fmt::Display for MessageHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

/// Public key compressed (33 bytes = 32 bytes + a y parity byte)
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct KeyId(pub Vec<u8>); // TODO: Use [u8; 33] not a Vec

impl std::fmt::Display for KeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MultisigRequest {
    Keygen(KeygenRequest),
    Sign(SigningRequest),
}

/// Start the multisig client, which listens for p2p messages and requests from the SC
pub fn start_client<S>(
    my_account_id: AccountId,
    db: S,
    mut multisig_request_receiver: UnboundedReceiver<MultisigRequest>,
    multisig_outcome_sender: UnboundedSender<MultisigOutcome>,
    mut incoming_p2p_message_receiver: UnboundedReceiver<(AccountId, MultisigMessage)>,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    keygen_options: KeygenOptions,
    logger: &slog::Logger,
) -> impl futures::Future
where
    S: KeyDB,
{
    let logger = logger.new(o!(COMPONENT_KEY => "MultisigClient"));

    slog::info!(logger, "Starting");

    let (inner_multisig_outcome_sender, mut inner_multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (keygen_request_sender, mut keygen_request_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (signing_request_sender, mut signing_request_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let mut client = MultisigClient::new(
        my_account_id.clone(),
        db,
        multisig_outcome_sender.clone(),
        keygen_request_sender,
        signing_request_sender,
        keygen_options,
        &logger,
    );

    use crate::multisig::client::ceremony_manager::CeremonyManager;

    let mut ceremony_manager = CeremonyManager::new(
        my_account_id,
        inner_multisig_outcome_sender,
        outgoing_p2p_message_sender,
        &logger,
    );

    async move {
        // Stream outputs () approximately every ten seconds
        let mut cleanup_tick = common::make_periodic_tick(Duration::from_secs(10));

        use rand_legacy::FromEntropy;
        let mut rng = crypto::Rng::from_entropy();

        loop {
            tokio::select! {
                Some((sender_id, message)) = incoming_p2p_message_receiver.recv() => {
                    ceremony_manager.process_p2p_message(sender_id, message);
                }
                Some(msg) = multisig_request_receiver.recv() => {
                    client.process_multisig_request(msg, &mut rng);
                }
                _ = cleanup_tick.tick() => {
                    slog::trace!(logger, "Checking for expired multisig states");
                    ceremony_manager.cleanup();
                    client.cleanup();
                }
                Some((rng, request, options)) = keygen_request_receiver.recv() => {
                    ceremony_manager.on_keygen_request(rng, request, options);
                }
                Some((rng, data, keygen_result_info, signers, ceremony_id)) = signing_request_receiver.recv() => {
                    ceremony_manager.on_request_to_sign(rng, data, keygen_result_info, signers, ceremony_id);
                }
                Some(multisig_outcome) = inner_multisig_outcome_receiver.recv() => {
                    match multisig_outcome {
                        MultisigOutcome::Signing(outcome) => {
                            multisig_outcome_sender.send(MultisigOutcome::Signing(outcome)).unwrap();
                        },
                        MultisigOutcome::Keygen(outcome) => {
                            if let Ok(keygen_result_info) = &outcome.result {
                                client.on_key_generated(keygen_result_info.clone());
                            }
                            multisig_outcome_sender.send(MultisigOutcome::Keygen(outcome)).unwrap();
                        },
                    }
                }
            }
        }
    }
}
