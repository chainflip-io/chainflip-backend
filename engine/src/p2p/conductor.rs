use slog::o;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_stream::StreamExt;

use crate::{
    logging::COMPONENT_KEY,
    p2p::{P2PRpcClient, P2PRpcEventHandler},
};

use super::{NetworkEventHandler, P2PMessage, P2PNetworkClient};

/// Drives P2P events between channels and P2P interface
// TODO: Can we just remove the conductor now that we have channels?
pub fn start(
    p2p: P2PRpcClient,
    p2p_message_sender: UnboundedSender<P2PMessage>,
    p2p_message_command_receiver: UnboundedReceiver<P2PMessage>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    logger: &slog::Logger,
) -> impl futures::Future {
    start_with_handler(
        P2PRpcEventHandler {
            p2p_message_sender,
            logger: logger.clone(),
        },
        p2p,
        p2p_message_command_receiver,
        shutdown_rx,
        logger,
    )
}

/// Start with a custom network event handler. Useful for mocks / testing.
pub(crate) fn start_with_handler<P2P, H>(
    network_event_handler: H,
    p2p: P2P,
    mut p2p_message_command_receiver: UnboundedReceiver<P2PMessage>,
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
                Some(P2PMessage { account_id, data }) = p2p_message_command_receiver.recv() => {
                    p2p.send(&account_id, &data).await.expect("Could not send outgoing P2PMessage");
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
