use slog::o;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::StreamExt;

use crate::{
    logging::COMPONENT_KEY,
    p2p::{P2PRpcClient, P2PRpcEventHandler},
};

use super::{NetworkEventHandler, P2PMessageCommand, P2PNetworkClient};

/// Drives P2P events between channels and P2P interface
// TODO: Can this be refactored now that we use channels
pub fn start(
    p2p: P2PRpcClient,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    logger: &slog::Logger,
) -> impl futures::Future {
    start_with_handler(
        P2PRpcEventHandler {
            mq: mq.clone(),
            logger: logger.clone(),
        },
        p2p,
        mq,
        shutdown_rx,
        &logger,
    )
}

/// Start with a custom network event handler. Useful for mocks / testing.
pub(crate) fn start_with_handler<P2P, H>(
    network_event_handler: H,
    p2p: P2P,
    p2p_message_command_receiver: UnboundedReceiver<P2PMessageCommand>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    logger: &slog::Logger,
) -> impl futures::Future
where
    P2P: P2PNetworkClient + Send,
    H: NetworkEventHandler<P2P>,
{
    let logger = logger.new(o!(COMPONENT_KEY => "P2PConductor"));

    async move {
        slog::info!(logger, "Starting");

        let mut p2p_event_stream = p2p.take_stream().await.expect("Should have p2p stream");

        loop {
            tokio::select! {
                Some(outgoing) = p2p_message_command_receiver.recv() => {
                    if let Ok(P2PMessageCommand { destination, data }) = outgoing {
                        p2p.send(&destination, &data).await.expect("Could not send outgoing P2PMessageCommand");
                    }
                }
                Some(incoming) = p2p_event_stream.next() => {
                    network_event_handler.handle_event(incoming).await;
                }
                Ok(()) = &mut shutdown_rx =>{
                    slog::info!(logger, "Shutting down");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use crate::{
        logging,
        mq::mq_mock::MQMock,
        p2p::{
            mock::{MockChannelEventHandler, NetworkMock},
            P2PMessageCommand, ValidatorId,
        },
    };

    use super::*;

    use tokio::time::timeout;

    #[tokio::test]
    async fn conductor_reads_from_mq() {
        use crate::mq::Subject;

        let network = NetworkMock::new();

        let logger = logging::test_utils::create_test_logger();

        // NOTE: for some reason connecting to the mock nat's server
        // is slow (0.5-1 seconds), which will add up when we have a
        // lot of tests. Will need to fix this.

        // Validator 1 setup
        let id_1: ValidatorId = ValidatorId([1; 32]);

        let mq = MQMock::new();

        let mc1 = mq.get_client();
        let mc1_copy = mq.get_client();
        let p2p_client_1 = network.new_client(id_1);
        let (handler_1, _) = MockChannelEventHandler::new();

        // Validator 2 setup
        let id_2: ValidatorId = ValidatorId([2; 32]);

        let mq = MQMock::new();
        let mc2 = mq.get_client();
        let p2p_client_2 = network.new_client(id_2.clone());
        let (handler_2, mut receiver) = MockChannelEventHandler::new();

        let (_, shutdown_conductor1_rx) = tokio::sync::oneshot::channel::<()>();
        let (_, shutdown_conductor2_rx) = tokio::sync::oneshot::channel::<()>();

        let conductor_fut_1 = timeout(
            Duration::from_millis(100),
            start_with_handler(
                handler_1,
                p2p_client_1,
                mc1,
                shutdown_conductor1_rx,
                &logger,
            ),
        );
        let conductor_fut_2 = timeout(
            Duration::from_millis(100),
            start_with_handler(
                handler_2,
                p2p_client_2,
                mc2,
                shutdown_conductor2_rx,
                &logger,
            ),
        );

        let msg_sent = b"hello";
        let cmd = P2PMessageCommand {
            destination: id_2,
            data: msg_sent.to_vec(),
        };

        let write_fut = async move {
            mc1_copy
                .publish(Subject::P2POutgoing, &cmd)
                .await
                .expect("Could not publish incoming P2PMessageCommand to Subject::P2POutgoing");
        };

        let read_fut = async move {
            // Second client should be able to receive the message
            let received = timeout(Duration::from_millis(100), receiver.recv())
                .await
                .expect("recv timeout")
                .expect("channel closed");

            assert_eq!(received.data, msg_sent);
        };

        let _ = futures::join!(conductor_fut_1, conductor_fut_2, write_fut, read_fut);
    }
}
