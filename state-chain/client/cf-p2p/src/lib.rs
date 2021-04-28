use std::collections::HashMap;
use std::hash::Hash;
use futures::{Stream, Future, StreamExt};
use std::task::{Context, Poll};
use std::pin::Pin;
use sc_network::{PeerId, Event, NetworkService, ExHashT};
use sp_runtime::{traits::Block as BlockT};
use sp_runtime::sp_std::sync::Arc;
use std::borrow::Cow;

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
}

impl<C, B, H> StateMachine<C, B, H>
    where
        C: Client,
        B: BlockT,
        H: ExHashT,
{
    pub fn new(client: C, network: Arc<NetworkService<B, H>>) -> Self {
        StateMachine {
            client,
            network,
            peers: HashMap::new(),
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

    pub fn send_message(&self, _peer_id: &PeerId, _data: Message) {
        unimplemented!()
    }

    pub fn broadcast(&self, _data: Message) {
        unimplemented!()
    }
 }

pub struct NetworkBridge<C: Client, B: BlockT, H: ExHashT> {
    network: Arc<NetworkService<B, H>>,
    state_machine: StateMachine<C, B, H>,
    network_event_stream: Pin<Box<dyn Stream<Item = Event> + Send>>,
    protocol: Cow<'static, str>,
}

impl<C, B, H> NetworkBridge<C, B, H>
    where
        C: Client,
        B: BlockT,
        H: ExHashT,
{
    pub fn new(client: C, network: Arc<NetworkService<B, H>>) -> Self {
        let state_machine = StateMachine::new(client, network.clone());
        let network_event_stream = Box::pin(network.event_stream("chainflip-cf-p2p"));
        NetworkBridge {
            network: network.clone(),
            state_machine,
            network_event_stream,
            protocol: "chainflip-cf-p2p".into(),
        }
    }

    pub fn send_message(&self, peer_id: &PeerId, data: Message) {
        self.state_machine.send_message(peer_id, data);
    }

    pub fn broadcast(&self, data: Message) {
        self.state_machine.broadcast(data);
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

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;

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
