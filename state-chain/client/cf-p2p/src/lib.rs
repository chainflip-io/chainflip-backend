use std::collections::HashMap;
use futures::{Stream, Future, StreamExt};
use std::task::{Context, Poll};
use std::pin::Pin;
use sc_network::{PeerId, Event, NetworkService, ExHashT};
use sp_runtime::{traits::Block as BlockT};
use sp_runtime::sp_std::sync::Arc;
use std::borrow::Cow;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use log::debug;
pub type Message = Vec<u8>;

pub trait Client {
    fn new_peer(&self, peer_id: &PeerId);
    fn disconnected(&self, peer_id: &PeerId);
    fn received(&self, peer_id: &PeerId, messages: Message);
}

impl Client for () {
    fn new_peer(&self, peer_id: &PeerId) {}
    fn disconnected(&self, peer_id: &PeerId) {}
    fn received(&self, peer_id: &PeerId, messages: Message) {}
}

struct StateMachine<C: Client, B: BlockT, H: ExHashT> {
    client: C,
    network: Arc<NetworkService<B, H>>,
    peers: HashMap<PeerId, ()>,
    protocol: Cow<'static, str>,
}

impl<C, B, H> StateMachine<C, B, H>
    where
        C: Client,
        B: BlockT,
        H: ExHashT,
{
    pub fn new(client: C, network: Arc<NetworkService<B, H>>, protocol: &'static str) -> Self {
        StateMachine {
            client,
            network,
            peers: HashMap::new(),
            protocol: protocol.into(),
        }
    }

    pub fn new_peer(&mut self, peer_id: &PeerId) {
        self.peers.insert(peer_id.clone(), ());
        self.client.new_peer(peer_id);
    }

    pub fn disconnected(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
        self.client.disconnected(peer_id);
    }

    pub fn received(&self, peer_id: &PeerId, messages: Vec<Message>) {
        for message in messages {
            self.client.received(peer_id, message);
        }
    }

    pub fn send_message(&mut self, peer_id: PeerId, message: Message) {
        if self.peers.contains_key(&peer_id) {
            self.network.write_notification(peer_id, self.protocol.clone(), message);
        }
    }

    pub fn broadcast(&self, _data: Message) {
        unimplemented!()
    }
}

struct OutgoingMessagesWorker {
    rx: UnboundedReceiver<(Vec<PeerId>, Message)>,
}

struct Sender(UnboundedSender<(Vec<PeerId>, Message)>);

impl Sender {
    fn send(&self, peer_id: PeerId, message: Message) {
        if let Err(e) = self.0.unbounded_send((vec![peer_id], message)) {
            debug!("Failed to send message {:?}", e);
        }
    }
}

impl Unpin for OutgoingMessagesWorker {}

impl OutgoingMessagesWorker {
    fn new() -> (Self, UnboundedSender<(Vec<PeerId>, Message)>) {
        let (tx, rx) = unbounded();
        (OutgoingMessagesWorker {
            rx,
        }, tx)
    }
}

impl Stream for OutgoingMessagesWorker {
    type Item = (Vec<PeerId>, Message);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        match this.rx.poll_next_unpin(cx) {
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Ready(Some((to, message))) => {
                return Poll::Ready(Some((to, message)));
            }
            Poll::Pending => {},
        };
        Poll::Pending
    }
}

pub struct NetworkBridge<C: Client, B: BlockT, H: ExHashT> {
    network: Arc<NetworkService<B, H>>,
    state_machine: StateMachine<C, B, H>,
    network_event_stream: Pin<Box<dyn Stream<Item = Event> + Send>>,
    protocol: Cow<'static, str>,
    worker: OutgoingMessagesWorker,
    sender: UnboundedSender<(Vec<PeerId>, Message)>
}

impl<C, B, H> NetworkBridge<C, B, H>
    where
        C: Client,
        B: BlockT,
        H: ExHashT,
{
    pub fn new(client: C, network: Arc<NetworkService<B, H>>) -> Self {
        let state_machine = StateMachine::new(client, network.clone(), "chainflip-cf-p2p");
        let network_event_stream = Box::pin(network.event_stream("chainflip-cf-p2p"));
        let (worker, sender) = OutgoingMessagesWorker::new();
        NetworkBridge {
            network: network.clone(),
            state_machine,
            network_event_stream,
            protocol: "chainflip-cf-p2p".into(),
            worker,
            sender,
        }
    }

    pub fn send_message(&mut self, peer_id: PeerId, data: Message) {
        if let Err(e) = self.sender.unbounded_send((vec![peer_id], data)) {
            debug!("Failed to push message to channel {:?}", e);
        }
    }

    pub fn broadcast(&self, data: Message) {
        todo!()
    }
}

impl<C, B, H> Unpin for NetworkBridge<C, B, H>
    where
        C: Client,
        B: BlockT,
        H: ExHashT {}


impl<C, B, H> Future for NetworkBridge<C, B, H>
    where
        C: Client,
        B: BlockT,
        H: ExHashT,
{
    type Output = ();

    // TODO return Result for poll
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        loop {
            match this.worker.poll_next_unpin(cx) {
                Poll::Ready(Some((peer_ids, message))) => {
                    for peer_id in peer_ids {
                        this.state_machine.send_message(peer_id, message.clone());
                    }
                },
                // TODO Handle close of stream
                Poll::Ready(None) => return Poll::Ready(
                    ()
                ),
                Poll::Pending => break,
            }
        }

        loop {
            match this.network_event_stream.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => match event {
                    Event::SyncConnected { remote } => {

                    }
                    Event::SyncDisconnected { remote } => {

                    }
                    Event::NotificationStreamOpened { remote, protocol, role } => {
                        if protocol != this.protocol {
                            continue;
                        }
                        this.state_machine.new_peer(&remote);
                    }
                    Event::NotificationStreamClosed { remote, protocol } => {
                        if protocol != this.protocol {
                            continue;
                        }
                        this.state_machine.disconnected(&remote);
                    },
                    Event::NotificationsReceived { remote, messages } => {
                        let messages: Vec<Message> = messages.into_iter().filter_map(|(engine, data)| {
                            if engine == this.protocol {
                                Some(data.to_vec())
                            } else {
                                None
                            }
                        }).collect();

                        this.state_machine.received(&remote, messages);
                    },
                    Event::Dht(_) => {}
                }
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Pending => break,
            }
        }

        Poll::Pending
    }
}

/// where's the bridge?
pub fn run_bridge<C: Client, B: BlockT, H: ExHashT>(client: C, network: Arc<NetworkService<B, H>>)
    -> impl Future<Output=()> {

    let network_bridge = NetworkBridge::new(client, network);
    network_bridge
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
