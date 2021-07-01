mod client_inner;

use std::{marker::PhantomData, time::Duration};

use crate::{
    mq::{pin_message_stream, IMQClient, IMQClientFactory, Subject},
    p2p::ValidatorId,
};
use anyhow::Result;
use futures::StreamExt;
use log::*;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::p2p::P2PMessage;

use self::client_inner::{InnerEvent, InnerSignal, KeygenOutcome, MultisigClientInner};

use super::{
    crypto::{Parameters, Signature},
    MessageHash, MessageInfo,
};

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
    ReadyToSign,
    MessageSigned(MessageInfo, Signature),
    KeygenResult(KeygenOutcome),
}

pub struct MultisigClient<MQ, F>
where
    MQ: IMQClient,
    F: IMQClientFactory<MQ>,
{
    factory: F,
    inner_event_receiver: Option<mpsc::UnboundedReceiver<InnerEvent>>,
    inner: MultisigClientInner,
    id: ValidatorId,
    _mq: PhantomData<MQ>,
}

// How long we keep individual signing phases around
// before expiring them
const PHASE_TIMEOUT: Duration = Duration::from_secs(20);

impl<MQ, F> MultisigClient<MQ, F>
where
    MQ: IMQClient,
    F: IMQClientFactory<MQ>,
{
    pub fn new(factory: F, id: ValidatorId, params: Parameters) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        MultisigClient {
            factory,
            inner: MultisigClientInner::new(id.clone(), params, tx, PHASE_TIMEOUT),
            inner_event_receiver: Some(rx),
            id,
            _mq: PhantomData,
        }
    }

    async fn process_inner_events(mut receiver: mpsc::UnboundedReceiver<InnerEvent>, mq: MQ) {
        while let Some(event) = receiver.recv().await {
            match event {
                InnerEvent::P2PMessageCommand(msg) => {
                    // TODO: do not send one by one
                    if let Err(err) = mq.publish(Subject::P2POutgoing, &msg).await {
                        error!("Could not publish message to MQ: {}", err);
                    }
                }
                InnerEvent::InnerSignal(InnerSignal::KeyReady) => {
                    mq.publish(Subject::MultisigEvent, &MultisigEvent::ReadyToSign)
                        .await
                        .expect("Signing module failed to publish readiness");
                }
                InnerEvent::InnerSignal(InnerSignal::MessageSigned(msg, sig)) => {
                    mq.publish(
                        Subject::MultisigEvent,
                        &MultisigEvent::MessageSigned(msg, sig),
                    )
                    .await
                    .expect("Failed to publish");
                }
                InnerEvent::KeygenResult(res) => {
                    mq.publish(Subject::MultisigEvent, &MultisigEvent::KeygenResult(res))
                        .await
                        .expect("Failed to publish");
                }
            }
        }
    }

    /// Start listening on the p2p connection and MQ
    pub async fn run(mut self) {
        let receiver = self.inner_event_receiver.take().unwrap();

        let mq = *self.factory.create().await.unwrap();

        let events_fut = MultisigClient::<_, F>::process_inner_events(receiver, mq);

        let (cleanup_tx, cleanup_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

        let cleanup_fut = async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                cleanup_tx.send(()).unwrap();
            }
        };

        let cleanup_stream = UnboundedReceiverStream::new(cleanup_rx);

        let mq = *self.factory.create().await.unwrap();

        let other_fut = async move {
            let stream1 = mq
                .subscribe::<P2PMessage>(Subject::P2PIncoming)
                .await
                .unwrap();

            let stream2 = mq
                .subscribe::<MultisigInstruction>(Subject::MultisigInstruction)
                .await
                .unwrap();

            // have to wait for the coordinator to subscribe...
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

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

            // TODO: call cleanup from time to time

            while let Some(msg) = stream_outer.next().await {
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

            error!("NO MORE MESSAGES");
        };

        futures::join!(events_fut, other_fut, cleanup_fut);
    }
}
