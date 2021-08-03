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

pub struct MultisigClient<MQC, S>
where
    MQC: IMQClient + Clone,
    S: KeyDB,
{
    mq_client: MQC,
    inner_event_receiver: Option<mpsc::UnboundedReceiver<InnerEvent>>,
    inner: MultisigClientInner<S>,
    my_validator_id: ValidatorId,
    logger: slog::Logger,
}

// How long we keep individual signing phases around
// before expiring them
const PHASE_TIMEOUT: Duration = Duration::from_secs(20);

impl<MQC, S> MultisigClient<MQC, S>
where
    MQC: IMQClient + Clone,
    S: KeyDB,
{
    pub fn new(db: S, mq_client: MQC, my_validator_id: ValidatorId, logger: &slog::Logger) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        Self {
            mq_client,
            inner: MultisigClientInner::new(my_validator_id.clone(), db, tx, PHASE_TIMEOUT, logger),
            inner_event_receiver: Some(rx),
            my_validator_id,
            logger: logger.new(o!(SIGNING_SUB_COMPONENT => "MultisigClient")),
        }
    }

    async fn process_inner_events(
        mut receiver: mpsc::UnboundedReceiver<InnerEvent>,
        mq: MQC,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
        logger: slog::Logger,
    ) {
        loop {
            tokio::select! {
                Some(event) = receiver.recv() =>{
                    match event {
                        InnerEvent::P2PMessageCommand(msg) => {
                            // TODO: do not send one by one
                            if let Err(err) = mq.publish(Subject::P2POutgoing, &msg).await {
                                slog::error!(logger, "Could not publish message to MQ: {}", err);
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
                Ok(()) = &mut shutdown_rx =>{
                    slog::info!(logger, "Shuting down Multisig Client InnerEvent loop");
                    break;
                }
            }
        }
    }

    /// Start listening on the p2p connection and MQ
    pub async fn run(mut self, mut shutdown_rx: tokio::sync::oneshot::Receiver<()>) {
        slog::info!(self.logger, "Starting");
        let receiver = self.inner_event_receiver.take().unwrap();

        let (cleanup_tx, cleanup_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

        let (shutdown_other_fut_tx, mut shutdown_other_fut_rx) =
            tokio::sync::oneshot::channel::<()>();

        let (shutdown_events_fut_tx, shutdown_events_fut_rx) =
            tokio::sync::oneshot::channel::<()>();

        let events_fut = MultisigClient::<_, S>::process_inner_events(
            receiver,
            self.mq_client.clone(),
            shutdown_events_fut_rx,
            self.logger.clone(),
        );

        let logger_c = self.logger.clone();
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

        let mq = self.mq_client.clone();

        let logger_c = self.logger.clone();
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

            slog::trace!(self.logger, "[{:?}] subscribed to MQ", self.my_validator_id);

            loop {
                tokio::select! {
                    Some(msg) = p2p_messages.next() => {
                        match msg {
                            Ok(p2p_message) => {
                                self.inner.process_p2p_mq_message(p2p_message);
                            },
                            Err(err) => {
                                slog::warn!(self.logger, "Ignoring channel error: {}", err);
                            }
                        }
                    }
                    Some(msg) = multisig_instructions.next() => {
                        match msg {
                            Ok(instruction) => {
                                self.inner.process_multisig_instruction(instruction);
                            },
                            Err(err) => {
                                slog::warn!(self.logger, "Ignoring channel error: {}", err);
                            }
                        }
                    }
                    Some(()) = cleanup_stream.next() => {
                        slog::info!(self.logger, "Cleaning up multisig states");
                        self.inner.cleanup();
                    }
                    Ok(()) = &mut shutdown_other_fut_rx =>{
                        slog::info!(self.logger, "Shutting down Multisig Client OtherEvents loop");
                        break;
                    }
                }
            }
        };
        futures::join!(events_fut, other_fut, cleanup_fut);
        slog::error!(logger_c, "MultisigClient stopped!");
    }
}
