//! Multisig signing and keygen

/// Multisig client
pub mod client;
/// Provides cryptographic primitives used by the multisig client
mod crypto;
/// Storage for the keys
pub mod db;

pub use crypto::{eth, ChainTag, Rng};

#[cfg(test)]
mod tests;

use cf_traits::CeremonyId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use serde::{Deserialize, Serialize};
use utilities::make_periodic_tick;

use std::{sync::Arc, time::Duration};

use crate::{
    logging::COMPONENT_KEY, multisig::client::CeremonyRequestDetails,
    multisig_p2p::OutgoingMultisigStageMessages,
};
use slog::o;
use state_chain_runtime::AccountId;

pub use client::{MultisigClient, MultisigMessage};

pub use db::PersistentKeyDB;

use self::{client::key_store::KeyStore, crypto::CryptoScheme};

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
pub fn start_client<C>(
    my_account_id: AccountId,
    key_store: KeyStore<C>,
    mut incoming_p2p_message_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    latest_ceremony_id: CeremonyId,
    logger: &slog::Logger,
) -> (
    Arc<MultisigClient<C>>,
    impl futures::Future<Output = ()> + Send,
)
where
    C: CryptoScheme,
{
    let logger = logger.new(o!(COMPONENT_KEY => "MultisigClient"));

    slog::info!(logger, "Starting");

    let (ceremony_request_sender, mut ceremony_request_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let multisig_client = Arc::new(MultisigClient::new(
        my_account_id.clone(),
        key_store,
        ceremony_request_sender,
        &logger,
    ));

    let multisig_client_backend_future = {
        use crate::multisig::client::ceremony_manager::CeremonyManager;

        let mut ceremony_manager = CeremonyManager::<C>::new(
            my_account_id,
            outgoing_p2p_message_sender,
            latest_ceremony_id,
            &logger,
        );

        async move {
            // Stream outputs () approximately every ten seconds
            let mut check_timeouts_tick = make_periodic_tick(Duration::from_secs(10), false);

            loop {
                tokio::select! {
                        Some(request) = ceremony_request_receiver.recv() => {
                            // Always update the latest ceremony id, even if we are not participating
                            ceremony_manager.update_latest_ceremony_id(request.ceremony_id);

                            match request.details {
                                Some(CeremonyRequestDetails::Keygen(details)) => {
                                    ceremony_manager.on_keygen_request(
                                        request.ceremony_id,
                                        details.participants,
                                        details.rng,
                                        details.result_sender,
                                    )
                                }
                                Some(CeremonyRequestDetails::Sign(details)) =>{
                                    ceremony_manager.on_request_to_sign(
                                        request.ceremony_id,
                                        details.participants,
                                        details.data,
                                        details.keygen_result_info,
                                        details.rng,
                                        details.result_sender,
                                );
                                }
                                None => { /* Not participating in the ceremony, so do nothing */ }
                            }
                    }

                    Some((sender_id, data)) = incoming_p2p_message_receiver.recv() => {

                        // For now we assume that every message we receive via p2p is a
                        // secp256k1 MultisigMessage (same as before). We will add
                        // demultiplexing once we add support for other types of messages.

                        match bincode::deserialize(&data) {
                            Ok(message) => ceremony_manager.process_p2p_message(sender_id, message),
                            Err(_) => {
                                slog::warn!(logger, "Failed to deserialize message from: {}", sender_id);
                            },
                        }

                    }
                    _ = check_timeouts_tick.tick() => {
                        slog::trace!(logger, "Checking for expired multisig states");
                        ceremony_manager.check_all_timeouts();
                    }
                }
            }
        }
    };

    (multisig_client, multisig_client_backend_future)
}
