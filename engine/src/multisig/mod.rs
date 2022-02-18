//! Multisig signing and keygen

/// Multisig client
mod client;
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

pub use self::client::{keygen::KeygenInfo, signing::SigningInfo};

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
pub enum MultisigInstruction {
    Keygen(KeygenInfo),
    Sign(SigningInfo),
}

/// Start the multisig client, which listens for p2p messages and instructions from the SC
pub fn start_client<S>(
    my_account_id: AccountId,
    db: S,
    mut multisig_instruction_receiver: UnboundedReceiver<MultisigInstruction>,
    multisig_outcome_sender: UnboundedSender<MultisigOutcome>,
    mut incoming_p2p_message_receiver: UnboundedReceiver<(AccountId, MultisigMessage)>,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    keygen_options: KeygenOptions,
    logger: &slog::Logger,
) -> impl futures::Future
where
    S: KeyDB,
{
    let logger = logger.new(o!(COMPONENT_KEY => "MultisigClient"));

    slog::info!(logger, "Starting");

    let mut client = MultisigClient::new(
        my_account_id,
        db,
        multisig_outcome_sender,
        outgoing_p2p_message_sender,
        keygen_options,
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
                    client.process_p2p_message(sender_id, message);
                }
                Some(msg) = multisig_instruction_receiver.recv() => {
                    client.process_multisig_instruction(msg, &mut rng);
                }
                _ = cleanup_tick.tick() => {
                    client.cleanup();
                }
                Ok(()) = &mut shutdown_rx => {
                    slog::info!(logger, "MultisigClient stopped due to shutdown request!");
                    break;
                }
            }
        }
    }
}
