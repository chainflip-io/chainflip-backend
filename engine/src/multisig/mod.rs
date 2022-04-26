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

use std::{sync::Arc, time::Duration};

use crate::{common, logging::COMPONENT_KEY, multisig_p2p::OutgoingMultisigStageMessages};
use slog::o;
use state_chain_runtime::AccountId;

pub use client::{KeygenOptions, MultisigClient, MultisigMessage, SchnorrSignature};

pub use db::{KeyDB, PersistentKeyDB};

#[cfg(test)]
pub use db::KeyDBMock;

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

/// Start the multisig client, which listens for p2p messages and requests from the SC
pub fn start_client<S>(
    my_account_id: AccountId,
    db: S,
    mut incoming_p2p_message_receiver: UnboundedReceiver<(AccountId, MultisigMessage)>,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    keygen_options: KeygenOptions,
    logger: &slog::Logger,
) -> (Arc<MultisigClient<S>>, impl futures::Future)
where
    S: KeyDB,
{
    let logger = logger.new(o!(COMPONENT_KEY => "MultisigClient"));

    slog::info!(logger, "Starting");

    let (keygen_request_sender, mut keygen_request_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (signing_request_sender, mut signing_request_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let multisig_client = Arc::new(MultisigClient::new(
        my_account_id.clone(),
        db,
        keygen_request_sender,
        signing_request_sender,
        keygen_options,
        &logger,
    ));

    let multisig_client_backend_future = {
        use crate::multisig::client::ceremony_manager::CeremonyManager;

        let mut ceremony_manager =
            CeremonyManager::new(my_account_id, outgoing_p2p_message_sender, &logger);

        async move {
            // Stream outputs () approximately every ten seconds
            let mut check_timeouts_tick = common::make_periodic_tick(Duration::from_secs(10));

            loop {
                tokio::select! {
                    Some((ceremony_id, participants, keygen_options, rng, result_sender)) = keygen_request_receiver.recv() => {
                        ceremony_manager.on_keygen_request(ceremony_id, participants, keygen_options, rng, result_sender);
                    }
                    Some((ceremony_id, signers, message_hash, keygen_result_info, rng, result_sender)) = signing_request_receiver.recv() => {
                        ceremony_manager.on_request_to_sign(ceremony_id, signers, message_hash, keygen_result_info, rng, result_sender);
                    }
                    Some((sender_id, message)) = incoming_p2p_message_receiver.recv() => {
                        ceremony_manager.process_p2p_message(sender_id, message);
                    }
                    _ = check_timeouts_tick.tick() => {
                        slog::trace!(logger, "Checking for expired multisig states");
                        ceremony_manager.check_timeouts();
                    }
                }
            }
        }
    };

    (multisig_client, multisig_client_backend_future)
}
