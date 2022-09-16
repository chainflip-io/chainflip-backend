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

use anyhow::Result;
use cf_primitives::CeremonyId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::{logging::COMPONENT_KEY, multisig_p2p::OutgoingMultisigStageMessages};
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
    incoming_p2p_message_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    latest_ceremony_id: CeremonyId,
    logger: &slog::Logger,
) -> (
    Arc<MultisigClient<C>>,
    impl futures::Future<Output = Result<()>> + Send,
)
where
    C: CryptoScheme,
{
    let logger = logger.new(o!(COMPONENT_KEY => "MultisigClient"));

    slog::info!(logger, "Starting");

    let (ceremony_request_sender, ceremony_request_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let multisig_client = Arc::new(MultisigClient::new(
        my_account_id.clone(),
        key_store,
        ceremony_request_sender,
        &logger,
    ));

    let multisig_client_backend_future = {
        use crate::multisig::client::ceremony_manager::CeremonyManager;

        let ceremony_manager = CeremonyManager::<C>::new(
            my_account_id,
            outgoing_p2p_message_sender,
            latest_ceremony_id,
            &logger,
        );

        ceremony_manager.run(ceremony_request_receiver, incoming_p2p_message_receiver)
    };

    (multisig_client, multisig_client_backend_future)
}
