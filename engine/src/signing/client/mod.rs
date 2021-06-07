mod client_inner;

use std::time::Duration;

use futures::{future::Either, StreamExt};
use log::*;

use crate::{
    mq::{pin_message_stream, IMQClient, Subject},
    p2p::P2PMessage,
    signing::client::client_inner::InnerSignal,
};

use self::client_inner::{InnerEvent, MultisigClientInner};

use super::{bitcoin_schnorr::Parameters, MessageHash};

use tokio::sync::mpsc;

// MultisigClient
// has two "big" states: KeyGen, Signing
// listens to multisig messages: KeyGenMessage, SigningMessage
// Rejects messages not for its current state
// We should probably save keygen messages just in case our node is behind, so we could process them later

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum MultisigInstruction {
    KeyGen,
    Sign(/* message */ Vec<u8>, /* signers */ Vec<usize>),
}

#[derive(Serialize, Deserialize)]
pub enum MultisigEvent {
    ReadyToKeygen,
    ReadyToSign,
    MessageSigned(MessageHash),
}

pub struct MultisigClient<MQ>
where
    MQ: IMQClient + Send + Sync + 'static,
{
    mq: MQ,
    mq2: Option<MQ>, // TODO: remove mq2
    inner_event_receiver: Option<mpsc::UnboundedReceiver<InnerEvent>>,
    signer_idx: usize,
    inner: MultisigClientInner,
}

// How long we keep individual signing phases around
// before expiring them
const PHASE_TIMEOUT: Duration = Duration::from_secs(20);

impl<MQ> MultisigClient<MQ>
where
    MQ: IMQClient + Send + Sync + 'static,
{
    // mq2 is used for sending p2p messages (TODO: pass in the server's address instead, so we
    // can create as many MQ clients as we want)
    pub fn new(mq: MQ, mq2: MQ, idx: usize, params: Parameters) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        MultisigClient {
            mq,
            mq2: Some(mq2),
            inner: MultisigClientInner::new(idx, params, tx, PHASE_TIMEOUT),
            signer_idx: idx,
            inner_event_receiver: Some(rx),
        }
    }

    async fn process_inner_events(mut receiver: mpsc::UnboundedReceiver<InnerEvent>, mq: MQ) {
        while let Some(event) = receiver.recv().await {
            match event {
                InnerEvent::P2PMessageCommand(msg) => {
                    // debug!("[{}] sending a message to [{}]", idx, msg.destination);
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
                InnerEvent::InnerSignal(InnerSignal::MessageSigned(msg)) => {
                    mq.publish(Subject::MultisigEvent, &MultisigEvent::MessageSigned(msg))
                        .await
                        .expect("Failed to publish");
                }
                _ => {
                    error!("Should process event");
                }
            }
        }
    }

    /// Start listening on the p2p connection
    pub async fn run(mut self) {
        // Should listen to:
        // - MQ messages
        // - p2p messages
        //   - p2p messages should be saved to a buffer
        //   - this module will process messages by reading the buffer

        let receiver = self.inner_event_receiver.take().unwrap();
        let mq = self.mq2.take().unwrap();

        let events_fut = MultisigClient::process_inner_events(receiver, mq);

        // let cleanup_fut = async move {

        //     loop {
        //         tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        //         self.inner.cleanup();
        //     }

        // };

        let other_fut = async move {
            let stream1 = self
                .mq
                .subscribe::<P2PMessage>(Subject::P2PIncoming)
                .await
                .unwrap();

            let stream2 = self
                .mq
                .subscribe::<MultisigInstruction>(Subject::MultisigInstruction)
                .await
                .unwrap();

            // have to wait for the coordinator to subscribe...
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

            // issue a message that we've subscribed
            self.mq
                .publish(Subject::MultisigEvent, &MultisigEvent::ReadyToKeygen)
                .await
                .expect("Signing module failed to publish readiness");

            let stream1 = pin_message_stream(stream1);

            let stream2 = pin_message_stream(stream2);

            let mut stream =
                futures::stream::select(stream1.map(Either::Left), stream2.map(Either::Right));

            trace!("[{}] subscribed to MQ", self.signer_idx);

            // TODO: call cleanup from time to time

            while let Some(msg) = stream.next().await {
                match msg {
                    Either::Left(Ok(p2p_message)) => {
                        self.inner.process_p2p_mq_message(p2p_message);
                    }
                    Either::Left(Err(err)) => {
                        warn!("Ignoring channel error: {}", err);
                    }
                    Either::Right(Ok(instruction)) => {
                        self.inner.process_multisig_instruction(instruction);
                    }
                    Either::Right(Err(err)) => {
                        warn!("Ignoring channel error: {}", err);
                    }
                }
            }

            error!("NO MORE MESSAGES");
        };

        futures::join!(events_fut, other_fut);
    }
}
