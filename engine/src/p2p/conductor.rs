use futures::{future::Either, Stream};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};

use crate::{
    mq::{pin_message_stream, IMQClient, Subject},
    p2p::P2PMessage,
};

use super::{P2PMessageCommand, P2PNetworkClient};

/// Intermediates P2P events between MQ and P2P interface
pub struct P2PConductor<MQ, P2P>
where
    MQ: IMQClient + Send,
    P2P: P2PNetworkClient,
{
    mq: MQ,
    p2p: P2P,
    stream: Box<dyn Stream<Item = Result<P2PMessageCommand, anyhow::Error>>>,
}

impl<MQ, P2P> P2PConductor<MQ, P2P>
where
    MQ: IMQClient + Send,
    P2P: P2PNetworkClient + Send,
{
    pub async fn new(mq: MQ, p2p: P2P) -> Self {
        let stream = mq
            .subscribe::<P2PMessageCommand>(Subject::P2POutgoing)
            .await
            .unwrap();

        P2PConductor { mq, p2p, stream }
    }

    pub async fn start(mut self) {
        type Msg = Either<Result<P2PMessageCommand, anyhow::Error>, P2PMessage>;

        let mq_stream = pin_message_stream(self.stream);

        let mq_stream = mq_stream.map(Msg::Left);

        let receiver = self.p2p.take_receiver().unwrap();

        let p2p_stream = UnboundedReceiverStream::new(receiver).map(Msg::Right);

        let mut stream = futures::stream::select(mq_stream, p2p_stream);

        while let Some(x) = stream.next().await {
            match x {
                Either::Left(outgoing) => {
                    if let Ok(P2PMessageCommand { destination, data }) = outgoing {
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

    use crate::{
        mq::mq_mock::MQMock,
        p2p::{mock::NetworkMock, P2PMessageCommand, ValidatorId},
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
        let id_1: ValidatorId = ValidatorId::new(1);

        let mq = MQMock::new();

        let mc1 = mq.get_client();
        let mc1_copy = mq.get_client();
        let p2p_client_1 = network.new_client(id_1);
        let conductor_1 = P2PConductor::new(mc1, p2p_client_1).await;

        // Validator 2 setup
        let id_2: ValidatorId = ValidatorId::new(2);

        let mq = MQMock::new();
        let mc2 = mq.get_client();
        let mc2_copy = mq.get_client();
        let p2p_client_2 = network.new_client(id_2.clone());
        let conductor_2 = P2PConductor::new(mc2, p2p_client_2).await;

        let conductor_fut_1 = timeout(Duration::from_millis(100), conductor_1.start());
        let conductor_fut_2 = timeout(Duration::from_millis(100), conductor_2.start());

        let msg = String::from("hello");

        let message = P2PMessageCommand {
            destination: id_2,
            data: Vec::from(msg.as_bytes()),
        };

        let write_fut = async move {
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

        let _ = futures::join!(conductor_fut_1, conductor_fut_2, write_fut, read_fut);
    }
}
