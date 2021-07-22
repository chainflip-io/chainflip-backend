mod client_inner;

use std::{marker::PhantomData, time::Duration};

use crate::{
    mq::{pin_message_stream, IMQClient, IMQClientFactory, Subject},
    p2p::ValidatorId,
    signing::db::KeyDB,
};
use anyhow::Result;
use futures::StreamExt;
use log::*;
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

pub struct MultisigClient<MQ, F, S>
where
    MQ: IMQClient,
    F: IMQClientFactory<MQ>,
    S: KeyDB,
{
    factory: F,
    inner_event_receiver: Option<mpsc::UnboundedReceiver<InnerEvent>>,
    inner: MultisigClientInner<S>,
    id: ValidatorId,
    _mq: PhantomData<MQ>,
}

// How long we keep individual signing phases around
// before expiring them
const PHASE_TIMEOUT: Duration = Duration::from_secs(20);

impl<MQ, F, S> MultisigClient<MQ, F, S>
where
    MQ: IMQClient,
    F: IMQClientFactory<MQ>,
    S: KeyDB,
{
    pub fn new(db: S, factory: F, id: ValidatorId) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        MultisigClient {
            factory,
            inner: MultisigClientInner::new(id.clone(), db, tx, PHASE_TIMEOUT),
            inner_event_receiver: Some(rx),
            id,
            _mq: PhantomData,
        }
    }

    async fn process_inner_events(
        mut receiver: mpsc::UnboundedReceiver<InnerEvent>,
        mq: MQ,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                Some(event) = receiver.recv() =>{
                    match event {
                        InnerEvent::P2PMessageCommand(msg) => {
                            // TODO: do not send one by one
                            if let Err(err) = mq.publish(Subject::P2POutgoing, &msg).await {
                                error!("Could not publish message to MQ: {}", err);
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
                Ok(()) = &mut shutdown_rx =>{log::info!("Shuting down Multisig Client InnerEvent loop");break;}
            }
        }
    }

    /// Start listening on the p2p connection and MQ
    pub async fn run(mut self, mut shutdown_rx: tokio::sync::oneshot::Receiver<()>) {
        let receiver = self.inner_event_receiver.take().unwrap();

        let mq = *self.factory.create().await.unwrap();

        let (cleanup_tx, cleanup_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

        let (shutdown_other_fut_tx, mut shutdown_other_fut_rx) =
            tokio::sync::oneshot::channel::<()>();

        let (shutdown_events_fut_tx, shutdown_events_fut_rx) =
            tokio::sync::oneshot::channel::<()>();

        let events_fut =
            MultisigClient::<_, F, S>::process_inner_events(receiver, mq, shutdown_events_fut_rx);

        let cleanup_fut = async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) =>{
                        cleanup_tx.send(()).expect("Could not send periotic cleanup command");
                    }
                    Ok(()) = &mut shutdown_rx =>{
                        log::info!("Shuting down Multisig Client");
                        // send a signal to the other futures to shutdown
                        shutdown_other_fut_tx.send(()).expect("Could not send shutdown command");
                        shutdown_events_fut_tx.send(()).expect("Could not send shutdown command");
                        break;
                    }
                }
            }
        };

        let cleanup_stream = UnboundedReceiverStream::new(cleanup_rx);

        let mq = *self.factory.create().await.unwrap();

        let other_fut = async move {
            let stream1 = mq
                .subscribe::<P2PMessage>(Subject::P2PIncoming)
                .await
                .expect("Could not subscribe to Subject::P2PIncoming");

            let stream2 = mq
                .subscribe::<MultisigInstruction>(Subject::MultisigInstruction)
                .await
                .expect("Could not subscribe to Subject::MultisigInstruction");

            // have to wait for the coordinator to subscribe...
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // issue a message that we've subscribed
            mq.publish(Subject::MultisigEvent, &MultisigEvent::ReadyToKeygen)
                .await
                .expect("Signing module failed to publish readiness");

            enum OtherEvents {
                Instruction(Result<MultisigInstruction>),
                Cleanup(()),
            }

            enum Events {
                P2P(Result<P2PMessage>),
                Other(OtherEvents),
            }

            let stream1 = pin_message_stream(stream1);
            let stream2 = pin_message_stream(stream2);

            let s1 = stream1.map(Events::P2P);
            let s2 = stream2.map(|x| Events::Other(OtherEvents::Instruction(x)));
            let s3 = cleanup_stream.map(|_| Events::Other(OtherEvents::Cleanup(())));

            let stream_inner = futures::stream::select(s2, s3);
            let mut stream_outer = futures::stream::select(s1, stream_inner);

            trace!("[{:?}] subscribed to MQ", self.id);

            loop {
                tokio::select! {
                    Some(msg) = stream_outer.next() =>{
                        match msg {
                            Events::P2P(Ok(p2p_message)) => {
                                self.inner.process_p2p_mq_message(p2p_message);
                            }
                            Events::P2P(Err(err)) => {
                                warn!("Ignoring channel error: {}", err);
                            }
                            Events::Other(OtherEvents::Instruction(Ok(instruction))) => {
                                self.inner.process_multisig_instruction(instruction);
                            }
                            Events::Other(OtherEvents::Instruction(Err(err))) => {
                                warn!("Ignoring channel error: {}", err);
                            }
                            Events::Other(OtherEvents::Cleanup(())) => {
                                info!("Cleaning up multisig states");
                                self.inner.cleanup();
                            }
                        }
                    }
                    Ok(()) = &mut shutdown_other_fut_rx =>{
                        log::info!("Shuting down Multisig Client OtherEvents loop");
                        break;
                    }
                }
            }
        };

        futures::join!(events_fut, other_fut, cleanup_fut);
    }
}
