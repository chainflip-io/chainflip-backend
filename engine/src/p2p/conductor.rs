use slog::o;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_stream::StreamExt;

use crate::{
    logging::COMPONENT_KEY,
    p2p::{P2PRpcClient, P2PRpcEventHandler},
};

use super::{NetworkEventHandler, P2PMessage, P2PMessageCommand, P2PNetworkClient};

/// Drives P2P events between channels and P2P interface
// TODO: Can this be refactored now that we use channels
pub fn start(
    p2p: P2PRpcClient,
    p2p_message_sender: UnboundedSender<P2PMessage>,
    p2p_message_command_receiver: UnboundedReceiver<P2PMessageCommand>,
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
        &logger,
    )
}

/// Start with a custom network event handler. Useful for mocks / testing.
pub(crate) fn start_with_handler<P2P, H>(
    network_event_handler: H,
    p2p: P2P,
    mut p2p_message_command_receiver: UnboundedReceiver<P2PMessageCommand>,
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
                Some(P2PMessageCommand { destination, data }) = p2p_message_command_receiver.recv() => {
                    p2p.send(&destination, &data).await.expect("Could not send outgoing P2PMessageCommand");
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
