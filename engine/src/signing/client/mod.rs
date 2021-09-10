mod client_inner;

use std::time::Duration;

use crate::{
    logging::SIGNING_SUB_COMPONENT,
    p2p::{P2PMessageCommand, ValidatorId},
    signing::db::KeyDB,
};
use futures::StreamExt;
use pallet_cf_vaults::CeremonyId;
use slog::o;

use crate::p2p::P2PMessage;

use self::client_inner::{InnerEvent, MultisigClientInner};

pub use client_inner::{KeygenOutcome, KeygenResultInfo, SchnorrSignature, SigningOutcome};

use super::MessageHash;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use serde::{Deserialize, Serialize};

/// Public key compressed (33 bytes - 32 bytes + a y parity byte)
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct KeyId(pub Vec<u8>);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeygenInfo {
    ceremony_id: CeremonyId,
    signers: Vec<ValidatorId>,
}

impl KeygenInfo {
    pub fn new(ceremony_id: CeremonyId, signers: Vec<ValidatorId>) -> Self {
        KeygenInfo {
            ceremony_id,
            signers,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SigningInfo {
    key_id: KeyId,
    signers: Vec<ValidatorId>,
}

impl SigningInfo {
    pub fn new(key_id: KeyId, signers: Vec<ValidatorId>) -> Self {
        SigningInfo { key_id, signers }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MultisigInstruction {
    KeyGen(KeygenInfo),
    Sign(MessageHash, SigningInfo),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MultisigEvent {
    MessageSigningResult(SigningOutcome),
    KeygenResult(KeygenOutcome),
}

// How long we keep individual signing phases around
// before expiring them
const PHASE_TIMEOUT: Duration = Duration::from_secs(20);

/// Start listening for p2p messages and instructions from the SC
pub fn start<S>(
    my_validator_id: ValidatorId,
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
    let logger = logger.new(o!(SIGNING_SUB_COMPONENT => "MultisigClient"));

    slog::info!(logger, "Starting");

    let (inner_event_sender, mut inner_event_receiver) = mpsc::unbounded_channel();
    let mut inner = MultisigClientInner::new(
        my_validator_id.clone(),
        db,
        inner_event_sender,
        PHASE_TIMEOUT,
        &logger,
    );

    async move {
        // Stream outputs () approximately every ten seconds
        let mut cleanup_stream = Box::pin(futures::stream::unfold((), |()| async move {
            Some((tokio::time::sleep(Duration::from_secs(10)).await, ()))
        }));

        loop {
            tokio::select! {
                Some(p2p_message) = p2p_message_receiver.recv() => {
                    inner.process_p2p_message(p2p_message);
                }
                Some(msg) = multisig_instruction_receiver.recv() => {
                    inner.process_multisig_instruction(msg);
                }
                Some(()) = cleanup_stream.next() => {
                    slog::info!(logger, "Cleaning up multisig states");
                    inner.cleanup();
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
                    slog::info!(logger, "MultisigClient stopped!");
                    break;
                }
            }
        }
    }
}
