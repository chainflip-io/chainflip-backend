//! Multisig signing

/// Multisig client
mod client;
/// Provides cryptographic primitives used by the multisig client
mod crypto;
/// Storage for the keys
mod db;

#[cfg(test)]
mod tests;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use serde::{Deserialize, Serialize};

use std::time::Duration;

use crate::{
    logging::COMPONENT_KEY,
    p2p::{AccountId, P2PMessageCommand},
};
use futures::StreamExt;
use slog::o;

use crate::p2p::P2PMessage;

use client::InnerEvent;

pub use client::{KeygenOutcome, MultisigClient, SchnorrSignature, SigningOutcome};

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

/// Public key compressed (33 bytes - 32 bytes + a y parity byte)
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct KeyId(pub Vec<u8>);

#[derive(Debug, Serialize, Deserialize)]
pub enum MultisigInstruction {
    KeyGen(KeygenInfo),
    Sign(SigningInfo),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MultisigEvent {
    MessageSigningResult(SigningOutcome),
    KeygenResult(KeygenOutcome),
}

/// Start the multisig client, which listens for p2p messages and instructions from the SC
pub fn start_client<S>(
    my_account_id: AccountId,
    db: S,
    mut multisig_instruction_receiver: UnboundedReceiver<MultisigInstruction>,
    multisig_event_sender: UnboundedSender<MultisigEvent>,
    mut p2p_message_receiver: UnboundedReceiver<P2PMessage>,
    p2p_message_command_sender: UnboundedSender<P2PMessageCommand>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    logger: &slog::Logger,
) -> impl futures::Future
where
    S: KeyDB,
{
    let logger = logger.new(o!(COMPONENT_KEY => "MultisigClient"));

    slog::info!(logger, "Starting");

    let (inner_event_sender, mut inner_event_receiver) = mpsc::unbounded_channel();
    let mut client = MultisigClient::new(my_account_id, db, inner_event_sender, &logger);

    async move {
        // Stream outputs () approximately every ten seconds
        let mut cleanup_stream = Box::pin(futures::stream::unfold((), |()| async move {
            Some((tokio::time::sleep(Duration::from_secs(10)).await, ()))
        }));

        loop {
            tokio::select! {
                Some(p2p_message) = p2p_message_receiver.recv() => {
                    client.process_p2p_message(p2p_message);
                }
                Some(msg) = multisig_instruction_receiver.recv() => {
                    client.process_multisig_instruction(msg);
                }
                Some(()) = cleanup_stream.next() => {
                    slog::trace!(logger, "Cleaning up multisig states");
                    client.cleanup();
                }
                Some(event) = inner_event_receiver.recv() => { // TODO: This will be removed entirely in the future
                    match event {
                        InnerEvent::P2PMessageCommand(p2p_message_command) => {
                            p2p_message_command_sender.send(p2p_message_command).map_err(|_| "Receiver dropped").unwrap();
                        }
                        InnerEvent::SigningResult(res) => {
                            multisig_event_sender.send(MultisigEvent::MessageSigningResult(res)).map_err(|_| "Receiver dropped").unwrap();
                        }
                        InnerEvent::KeygenResult(res) => {
                            multisig_event_sender.send(MultisigEvent::KeygenResult(res)).map_err(|_| "Receiver dropped").unwrap();
                        }
                    }
                }
                Ok(()) = &mut shutdown_rx => {
                    slog::info!(logger, "MultisigClient stopped due to shutdown request!");
                    break;
                }
            }
        }
    }
}
