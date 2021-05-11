use std::pin::Pin;

use futures::{future::Either, Stream};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};

use crate::{
    mq::{pin_message_stream, IMQClient, Subject},
    p2p::{self, P2PMessage},
};

use super::{CommandSendMessage, P2PNetworkClient};

/// Intermediates P2P events between MQ and P2P interface
pub struct P2PConductor<MQ, P2P>
where
    MQ: IMQClient,
    P2P: P2PNetworkClient,
{
    mq: MQ,
    p2p: P2P,
    stream: Pin<Box<dyn Stream<Item = Result<CommandSendMessage, anyhow::Error>>>>,
}

impl<MQ, P2P> P2PConductor<MQ, P2P>
where
    MQ: IMQClient,
    P2P: P2PNetworkClient,
{
    pub async fn new(mq: MQ, p2p: P2P) -> Self {
        let stream = mq
            .subscribe::<CommandSendMessage>(Subject::P2POutgoing)
            .await
            .expect("P2P Conductor could not subscribe to MQ");

        let stream = pin_message_stream(stream);

        P2PConductor { mq, p2p, stream }
    }

    pub async fn start(mut self) {
        type Msg = Either<Result<CommandSendMessage, anyhow::Error>, P2PMessage>;

        let mq_stream = self.stream.map(|x| Msg::Left(x));

        let receiver = self.p2p.take_receiver().unwrap();

        let p2p_stream = UnboundedReceiverStream::new(receiver).map(|x| Msg::Right(x));

        let mut stream = futures::stream::select(mq_stream, p2p_stream);

        while let Some(x) = stream.next().await {
            match x {
                Either::Left(outgoing) => {
                    if let Ok(CommandSendMessage { destination, data }) = outgoing {
                        self.p2p.send(&destination, &data);
                    }
                }
                Either::Right(incoming) => {
                    self.mq
                        .publish(Subject::P2PIncoming, &incoming)
                        .await
                        .unwrap();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use nats_test_server::{NatsTestServer, NatsTestServerBuilder};
    use tokio_stream::wrappers::UnboundedReceiverStream;

    use crate::{
        mq::mq_mock::MockMQ,
        p2p::{mock::NetworkMock, CommandSendMessage, ValidatorId},
    };

    use super::*;

    use tokio::time::timeout;

    #[tokio::test]
    async fn conductor_reads_from_mq() {
        use crate::mq::Subject;

        let network = NetworkMock::new();

        // NOTE: for some reason connecting to the mock nat's server
        // is slow (0.5-1 seconds), which will add up when we have a
        // lot of tests. Will need to fix this.

        // Validator 1 setup
        const ID_1: ValidatorId = 1;
        let server = NatsTestServer::build().spawn();
        let mc1 = MockMQ::new(&server).await;
        let mc1_copy = MockMQ::new(&server).await;
        let p2p_client_1 = network.new_client(ID_1);
        let conductor_1 = P2PConductor::new(mc1, p2p_client_1).await;

        // Validator 2 setup
        const ID_2: ValidatorId = 2;
        let server2 = NatsTestServer::build().spawn();
        let mc2 = MockMQ::new(&server2).await;
        let mc2_copy = MockMQ::new(&server2).await;
        let p2p_client_2 = network.new_client(ID_2);
        let conductor_2 = P2PConductor::new(mc2, p2p_client_2).await;

        let conductor_fut_1 = timeout(Duration::from_millis(100), conductor_1.start());
        let conductor_fut_2 = timeout(Duration::from_millis(100), conductor_2.start());

        let msg = String::from("hello");

        let message = CommandSendMessage {
            destination: ID_2,
            data: Vec::from(msg.as_bytes()),
        };

        let write_fut = async move {
            // For whatever reason when NatsTestServer is used (does not seem to be
            // the case for the real nats server), there is a few millisecond window
            // right after I've subscribed to the stream before I actually start
            // getting messages (i.e. some messages might be dropped if I don't wait)
            tokio::time::sleep(Duration::from_millis(50)).await;

            mc1_copy
                .publish(Subject::P2POutgoing, &message)
                .await
                .unwrap();
        };

        let read_fut = async move {
            let stream2 = mc2_copy
                .subscribe::<P2PMessage>(Subject::P2PIncoming)
                .await
                .unwrap();

            let mut stream2 = pin_message_stream(stream2);

            // Second client should be able to receive the message
            let maybe_msg = timeout(Duration::from_millis(100), stream2.next()).await;

            assert!(maybe_msg.is_ok(), "recv timeout");

            assert_eq!(maybe_msg.unwrap().unwrap().unwrap().data, msg.as_bytes());
        };

        futures::join!(conductor_fut_1, conductor_fut_2, write_fut, read_fut);
    }
}
