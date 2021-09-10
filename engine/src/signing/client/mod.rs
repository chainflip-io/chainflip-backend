mod client_inner;

use std::time::Duration;

use crate::{
    logging::SIGNING_SUB_COMPONENT,
    mq::{IMQClient, Subject},
    p2p::AccountId,
    signing::db::KeyDB,
};
use futures::StreamExt;
use slog::o;

use crate::p2p::P2PMessage;

use self::client_inner::{InnerEvent, MultisigClientInner};

pub use client_inner::{KeygenOutcome, KeygenResultInfo, SchnorrSignature, SigningOutcome};

use super::MessageHash;

use tokio::sync::mpsc;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub struct KeyId(pub u64);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeygenInfo {
    id: KeyId,
    signers: Vec<AccountId>,
}

impl KeygenInfo {
    pub fn new(id: KeyId, signers: Vec<AccountId>) -> Self {
        KeygenInfo { id, signers }
    }
}

/// Note that this is different from `AuctionInfo` as
/// not every multisig party will participate in
/// any given ceremony
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SigningInfo {
    id: KeyId,
    signers: Vec<AccountId>,
}

impl SigningInfo {
    pub fn new(id: KeyId, signers: Vec<AccountId>) -> Self {
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
    my_validator_id: AccountId,
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

    slog::info!(logger, "Starting");

    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let mut inner = MultisigClientInner::new(
        my_validator_id.clone(),
        db,
        events_tx,
        PHASE_TIMEOUT,
        &logger,
    );

    async move {
        let mut p2p_messages = mq_client
            .subscribe::<P2PMessage>(Subject::P2PIncoming)
            .await
            .expect("Could not subscribe to Subject::P2PIncoming");

        let mut multisig_instructions = mq_client
            .subscribe::<MultisigInstruction>(Subject::MultisigInstruction)
            .await
            .expect("Could not subscribe to Subject::MultisigInstruction");

        {
            // have to wait for the coordinator to subscribe...
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // issue a message that we've subscribed
            mq_client
                .publish(Subject::MultisigEvent, &MultisigEvent::ReadyToKeygen)
                .await
                .expect("Signing module failed to publish readiness");

            slog::trace!(logger, "[{:?}] subscribed to MQ", my_validator_id);
        }

        // Stream outputs () approximately every ten seconds
        let mut cleanup_stream = Box::pin(futures::stream::unfold((), |()| async move {
            Some((tokio::time::sleep(Duration::from_secs(10)).await, ()))
        }));

        loop {
            tokio::select! {
                Some(msg) = p2p_messages.next() => {
                    match msg {
                        Ok(p2p_message) => {
                            inner.process_p2p_mq_message(p2p_message);
                        },
                        Err(err) => {
                            slog::warn!(logger, "Ignoring channel error: {}", err);
                        }
                    }
                }
                Some(msg) = multisig_instructions.next() => {
                    match msg {
                        Ok(instruction) => {
                            inner.process_multisig_instruction(instruction);
                        },
                        Err(err) => {
                            slog::warn!(logger, "Ignoring channel error: {}", err);
                        }
                    }
                }
                Some(()) = cleanup_stream.next() => {
                    slog::info!(logger, "Cleaning up multisig states");
                    inner.cleanup();
                }
                Some(event) = events_rx.recv() => { // TODO: This will be removed entirely in the future
                    match event {
                        InnerEvent::P2PMessageCommand(msg) => {
                            // TODO: do not send one by one
                            if let Err(err) = mq_client.publish(Subject::P2POutgoing, &msg).await {
                                slog::error!(logger, "Could not publish message to MQ: {}", err);
                            }
                        }
                        InnerEvent::SigningResult(res) => {
                            mq_client.publish(
                                Subject::MultisigEvent,
                                &MultisigEvent::MessageSigningResult(res),
                            )
                            .await
                            .expect("Failed to publish MessageSigningResult");
                        }
                        InnerEvent::KeygenResult(res) => {
                            mq_client.publish(Subject::MultisigEvent, &MultisigEvent::KeygenResult(res))
                                .await
                                .expect("Failed to publish KeygenResult");
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
