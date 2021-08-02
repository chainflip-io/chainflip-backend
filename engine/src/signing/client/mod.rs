mod client_inner;

use std::time::Duration;

use crate::{
    logging::SIGNING_SUB_COMPONENT,
    mq::{pin_message_stream, IMQClient, Subject},
    p2p::ValidatorId,
    signing::db::KeyDB,
};
use futures::StreamExt;
use slog::o;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::p2p::P2PMessage;

use self::client_inner::{InnerEvent, MultisigClientInner};

pub use client_inner::{
    KeygenOutcome, KeygenResultInfo, KeygenSuccess, SigningOutcome, SigningSuccess,
};

use super::MessageHash;

use tokio::sync::mpsc;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub struct KeyId(pub u64);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeygenInfo {
    id: KeyId,
    signers: Vec<ValidatorId>,
}

impl KeygenInfo {
    pub fn new(id: KeyId, signers: Vec<ValidatorId>) -> Self {
        KeygenInfo { id, signers }
    }
}

/// Note that this is different from `AuctionInfo` as
/// not every multisig party will participate in
/// any given ceremony
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SigningInfo {
    id: KeyId,
    signers: Vec<ValidatorId>,
}

impl SigningInfo {
    pub fn new(id: KeyId, signers: Vec<ValidatorId>) -> Self {
        SigningInfo { id, signers }
    }
}

#[derive(Serialize, Deserialize)]
pub enum MultisigInstruction {
    KeyGen(KeygenInfo),
    Sign(MessageHash, SigningInfo),
}

#[derive(Serialize, Deserialize)]
pub enum MultisigEvent {
    ReadyToKeygen,
    MessageSigningResult(SigningOutcome),
    KeygenResult(KeygenOutcome),
}

// How long we keep individual signing phases around
// before expiring them
const PHASE_TIMEOUT: Duration = Duration::from_secs(20);

/// Start listening on the p2p connection and MQ
pub fn start<MQC, S>(
    my_validator_id: ValidatorId,
    db: S,
    mq_client: MQC,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    logger: &slog::Logger,
) -> impl futures::Future
where
    MQC: IMQClient + Clone,
    S: KeyDB,
{
    let logger = logger.new(o!(SIGNING_SUB_COMPONENT => "MultisigClient"));
    async move {
        slog::info!(logger, "Starting");

        let (sender, mut receiver) = mpsc::unbounded_channel();
        let (cleanup_tx, cleanup_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let (shutdown_other_fut_tx, mut shutdown_other_fut_rx) =
            tokio::sync::oneshot::channel::<()>();
        let (shutdown_events_fut_tx, mut shutdown_events_fut_rx) =
            tokio::sync::oneshot::channel::<()>();

        let mq = mq_client.clone();
        let logger_c = logger.clone();
        let events_fut = async move {
            loop {
                tokio::select! {
                    Some(event) = receiver.recv() =>{
                        match event {
                            InnerEvent::P2PMessageCommand(msg) => {
                                // TODO: do not send one by one
                                if let Err(err) = mq.publish(Subject::P2POutgoing, &msg).await {
                                    slog::error!(logger_c, "Could not publish message to MQ: {}", err);
                                }
                            }
                            InnerEvent::SigningResult(res) => {
                                mq.publish(
                                    Subject::MultisigEvent,
                                    &MultisigEvent::MessageSigningResult(res),
                                )
                                .await
                                .expect("Failed to publish MessageSigningResult");
                            }
                            InnerEvent::KeygenResult(res) => {
                                mq.publish(Subject::MultisigEvent, &MultisigEvent::KeygenResult(res))
                                    .await
                                    .expect("Failed to publish KeygenResult");
                            }
                        }
                    }
                    Ok(()) = &mut shutdown_events_fut_rx =>{
                        slog::info!(logger_c, "Shuting down Multisig Client InnerEvent loop");
                        break;
                    }
                }
            }
        };

        let logger_c = logger.clone();
        let cleanup_fut = async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) =>{
                        cleanup_tx.send(()).expect("Could not send periotic cleanup command");
                    }
                    Ok(()) = &mut shutdown_rx =>{
                        slog::info!(logger_c, "Shutting down");
                        // send a signal to the other futures to shutdown
                        shutdown_other_fut_tx.send(()).expect("Could not send shutdown command");
                        shutdown_events_fut_tx.send(()).expect("Could not send shutdown command");
                        break;
                    }
                }
            }
        };

        let mut cleanup_stream = UnboundedReceiverStream::new(cleanup_rx);

        let mq = mq_client.clone();
        let logger_c = logger.clone();
        let mut inner =
            MultisigClientInner::new(my_validator_id.clone(), db, sender, PHASE_TIMEOUT, &logger);
        let other_fut = async move {
            let mut p2p_messages = pin_message_stream(
                mq.subscribe::<P2PMessage>(Subject::P2PIncoming)
                    .await
                    .expect("Could not subscribe to Subject::P2PIncoming"),
            );

            let mut multisig_instructions = pin_message_stream(
                mq.subscribe::<MultisigInstruction>(Subject::MultisigInstruction)
                    .await
                    .expect("Could not subscribe to Subject::MultisigInstruction"),
            );

            // have to wait for the coordinator to subscribe...
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // issue a message that we've subscribed
            mq.publish(Subject::MultisigEvent, &MultisigEvent::ReadyToKeygen)
                .await
                .expect("Signing module failed to publish readiness");

            slog::trace!(logger_c, "[{:?}] subscribed to MQ", my_validator_id);

            loop {
                tokio::select! {
                    Some(msg) = p2p_messages.next() => {
                        match msg {
                            Ok(p2p_message) => {
                                inner.process_p2p_mq_message(p2p_message);
                            },
                            Err(err) => {
                                slog::warn!(logger_c, "Ignoring channel error: {}", err);
                            }
                        }
                    }
                    Some(msg) = multisig_instructions.next() => {
                        match msg {
                            Ok(instruction) => {
                                inner.process_multisig_instruction(instruction);
                            },
                            Err(err) => {
                                slog::warn!(logger_c, "Ignoring channel error: {}", err);
                            }
                        }
                    }
                    Some(()) = cleanup_stream.next() => {
                        slog::info!(logger_c, "Cleaning up multisig states");
                        inner.cleanup();
                    }
                    Ok(()) = &mut shutdown_other_fut_rx =>{
                        slog::info!(logger_c, "Shutting down Multisig Client OtherEvents loop");
                        break;
                    }
                }
            }
        };
        futures::join!(events_fut, other_fut, cleanup_fut);
        slog::error!(logger, "MultisigClient stopped!");
    }
}
