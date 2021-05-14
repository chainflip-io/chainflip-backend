use std::collections::HashMap;
use futures::{Stream, Future, StreamExt};
use std::task::{Context, Poll};
use std::pin::Pin;
use sc_network::{PeerId, Event, NetworkService, ExHashT};
use sp_runtime::{traits::Block as BlockT};
use sp_runtime::sp_std::sync::Arc;
use std::borrow::Cow;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use log::{debug};
use std::sync::Mutex;

pub type Message = Vec<u8>;

pub trait PeerNetwork : Clone {
    /// Write notification to network to peer id, over protocol
    fn write_notification(&self, who: PeerId, protocol: Cow<'static, str>, message: Message);
    /// Network event stream
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>>;
}

impl<B: BlockT, H: ExHashT> PeerNetwork for Arc<NetworkService<B, H>> {
    fn write_notification(&self, target: PeerId, protocol: Cow<'static, str>, message: Message) {
        NetworkService::write_notification(self, target, protocol, message)
    }
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        Box::pin(NetworkService::event_stream(self, "network-chainflip"))
    }
}

/// Observing events when they arrive at this peer
pub trait NetworkObserver {
    /// On a new peer connected to the network
    fn new_peer(&self, peer_id: &PeerId);
    /// On a peer being disconnected
    fn disconnected(&self, peer_id: &PeerId);
    /// A message being received from peer_id for this peer
    fn received(&self, peer_id: &PeerId, messages: Message);
}

/// A state machine routing messages and events to our network and observer
struct StateMachine<Observer: NetworkObserver, Network: PeerNetwork> {
    /// A reference to an NetworkObserver
    observer: Arc<Observer>,
    /// The peer to peer network
    network: Network,
    /// List of peers on the network
    peers: HashMap<PeerId, ()>,
    /// The protocol's name
    protocol: Cow<'static, str>,
}

impl<Observer, Network> StateMachine<Observer, Network>
    where
        Observer: NetworkObserver,
        Network: PeerNetwork,
{
    pub fn new(observer: Arc<Observer>, network: Network, protocol: Cow<'static, str>) -> Self {
        StateMachine {
            observer,
            network,
            peers: HashMap::new(),
            protocol,
        }
    }

    /// A new peer has arrived, insert into our internal list and notify observer
    pub fn new_peer(&mut self, peer_id: &PeerId) {
        self.peers.insert(peer_id.clone(), ());
        self.observer.new_peer(peer_id);
    }

    /// A new peer has disconnected, insert into our internal list and notify observer
    pub fn disconnected(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
        self.observer.disconnected(peer_id);
    }

    /// Messages received from peer_id, notify observer
    pub fn received(&self, peer_id: &PeerId, messages: Vec<Message>) {
        for message in messages {
            self.observer.received(peer_id, message);
        }
    }

    /// Send message to peer, this will fail silently if peer isn't in our peer list or the message
    /// is empty
    pub fn send_message(&mut self, peer_id: PeerId, message: Message) {
        if self.peers.contains_key(&peer_id) && !message.is_empty() {
            self.network.write_notification(peer_id, self.protocol.clone(), message);
        }
    }

    /// Broadcast message to network, this will fail silently if the message is empty
    pub fn broadcast(&mut self, message: Message) {
        if !message.is_empty() {
            for peer_id in self.peers.keys() {
                self.network.write_notification(*peer_id, self.protocol.clone(), message.clone());
            }
        }
    }
}

/// The entry point.  The network bridge provides the trait `Messaging`.
pub struct NetworkBridge<Observer: NetworkObserver, Network: PeerNetwork> {
    state_machine: StateMachine<Observer, Network>,
    network_event_stream: Pin<Box<dyn Stream<Item = Event> + Send>>,
    protocol: Cow<'static, str>,
    worker: UnboundedReceiver<(Vec<PeerId>, Message)>,
}

impl<Observer, Network> NetworkBridge<Observer, Network>
    where
        Observer: NetworkObserver,
        Network: PeerNetwork,
{
    pub fn new(observer: Arc<Observer>, network: Network, protocol: Cow<'static, str>) -> (Self, Arc<Mutex<Sender>>) {
        let state_machine = StateMachine::new(observer, network.clone(), protocol.clone());
        let network_event_stream = Box::pin(network.event_stream());
        let (sender, worker) = unbounded::<(Vec<PeerId>, Message)>();
        let messenger = Arc::new(Mutex::new(Sender(sender.clone())));
        (NetworkBridge {
            state_machine,
            network_event_stream,
            protocol: protocol.clone(),
            worker,
        }, messenger.clone())
    }
}

/// Messaging by sending directly or broadcasting
pub trait Messaging {
    fn send_message(&mut self, peer_id: &PeerId, data: Message) -> bool;
    fn broadcast(&self, data: Message) -> bool;
}

/// Push messages down our channel to be past on to the network
pub struct Sender(UnboundedSender<(Vec<PeerId>, Message)>);
impl Messaging for Sender {
    fn send_message(&mut self, peer_id: &PeerId, data: Message) -> bool {
        if let Err(e) = self.0.unbounded_send((vec![*peer_id], data)) {
            debug!("Failed to push message to channel {:?}", e);
            return false
        }
        true
    }

    fn broadcast(&self, data: Message) -> bool {
        if let Err(e) = self.0.unbounded_send((vec![], data)) {
            debug!("Failed to broadcast message to channel {:?}", e);
            return false
        }
        true
    }
}

impl<Observer, Network> Unpin for NetworkBridge<Observer, Network>
    where
        Observer: NetworkObserver,
        Network: PeerNetwork, {}

/// `Future` for `NetworkBridge` - poll our outgoing messages and pass them to the `StateMachine` for sending
/// After which we poll the network for events and again back to the `StateMachine`
impl<Observer, Network> Future for NetworkBridge<Observer, Network>
    where
        Observer: NetworkObserver,
        Network: PeerNetwork,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        loop {
            match this.worker.poll_next_unpin(cx) {
                Poll::Ready(Some((peer_ids, message))) => {
                    if peer_ids.is_empty() {
                        this.state_machine.broadcast(message.clone());
                    } else {
                        for peer_id in peer_ids {
                            this.state_machine.send_message(peer_id, message.clone());
                        }
                    }
                },
                Poll::Ready(None) => return Poll::Ready(
                    ()
                ),
                Poll::Pending => break,
            }
        }

        loop {
            match this.network_event_stream.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => match event {
                    Event::SyncConnected { remote: _ } => {}
                    Event::SyncDisconnected { remote: _ } => {}
                    Event::NotificationStreamOpened { remote, protocol, role: _ } => {
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
                        if !messages.is_empty() {
                            let messages: Vec<Message> = messages.into_iter().filter_map(|(engine, data)| {
                                if engine == this.protocol {
                                    Some(data.to_vec())
                                } else {
                                    None
                                }
                            }).collect();

                            this.state_machine.received(&remote, messages);
                        }
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

#[cfg(test)]
mod tests {
    use futures::{channel::mpsc::{unbounded, UnboundedSender}, executor::{block_on}, future::poll_fn, FutureExt};
    use futures::{Stream};
    use sc_network::{PeerId, Event, ObservedRole};
    use std::sync::{Arc, Mutex};
    use super::*;

    #[derive(Clone, Default)]
    struct TestNetwork {
        inner: Arc<Mutex<TestNetworkInner>>,
    }

    #[derive(Clone, Default)]
    struct TestNetworkInner {
        event_senders: Vec<UnboundedSender<Event>>,
        notifications: Vec<(Vec<u8>, Vec<u8>)>,
    }

    impl PeerNetwork for TestNetwork {
        fn write_notification(&self, who: PeerId, _protocol: Cow<'static, str>, message: Vec<u8>) {
            self.inner.lock().unwrap().notifications.push((who.to_bytes(), message));
        }

        fn event_stream(&self) -> Pin<Box<dyn Stream<Item=Event> + Send>> {
            let (tx, rx) = unbounded();
            self.inner.lock().unwrap().event_senders.push(tx);

            Box::pin(rx)
        }
    }

    #[derive(Clone, Default)]
    struct TestObserver {
        inner: Arc<Mutex<TestObserverInner>>
    }

    #[derive(Clone, Default)]
    struct TestObserverInner(Option<Vec<u8>>,
                             Option<Vec<u8>>,
                             Option<(Vec<u8>, Vec<u8>)>);

    impl NetworkObserver for TestObserver {
        fn new_peer(&self, peer_id: &PeerId) {
            self.inner.lock().unwrap().0 = Some(peer_id.to_bytes());
        }

        fn disconnected(&self, peer_id: &PeerId) {
            self.inner.lock().unwrap().1 = Some(peer_id.to_bytes());
        }

        fn received(&self, peer_id: &PeerId, messages: Message) {
            self.inner.lock().unwrap().2 = Some((peer_id.to_bytes(), messages));
        }
    }

    #[test]
    fn send_message_to_peer() {
        let network = TestNetwork::default();
        let protocol = Cow::Borrowed("/chainflip-protocol");
        let observer = Arc::new(TestObserver::default());
        let (mut bridge, communications) = NetworkBridge::new(
            observer.clone(),
            network.clone(),
            protocol.clone());

        let peer = PeerId::random();

        // Register peer
        let mut event_sender = network.inner.lock()
            .unwrap()
            .event_senders
            .pop()
            .unwrap();

        let msg = Event::NotificationStreamOpened {
            remote: peer.clone(),
            protocol: protocol.clone(),
            role: ObservedRole::Authority,
        };

        event_sender.start_send(msg).expect("Event stream is unbounded");

        block_on(poll_fn(|cx| {
            let mut sent = false;
            loop {
                if let Poll::Ready(()) = bridge.poll_unpin(cx) {
                    unreachable!("we should have a new network event");
                }

                let o = observer.inner.lock().unwrap();

                if let Some(_) = &o.0  {
                    if !sent {
                        communications.lock().unwrap().send_message(&peer, b"this rocks".to_vec());
                        sent = true;
                    }

                    if let Some(notification) = network.inner.lock()
                        .unwrap().notifications.pop().as_ref() {
                        assert_eq!(notification.clone(), (peer.to_bytes(), b"this rocks".to_vec()));
                        break;
                    }
                }
            }
            Poll::Ready(())
        }));
    }

    #[test]
    fn broadcast_message_to_peers() {
        let network = TestNetwork::default();
        let protocol = Cow::Borrowed("/chainflip-protocol");
        let observer = Arc::new(TestObserver::default());
        let (mut bridge, comms) = NetworkBridge::new(
            observer.clone(),
            network.clone(),
            protocol.clone());

        let peer = PeerId::random();
        let peer_1 = PeerId::random();

        // Register peers
        let mut event_sender = network.inner.lock()
            .unwrap()
            .event_senders
            .pop()
            .unwrap();

        let msg = Event::NotificationStreamOpened {
            remote: peer.clone(),
            protocol: protocol.clone(),
            role: ObservedRole::Authority,
        };
        event_sender.start_send(msg).expect("Event stream is unbounded");

        let msg = Event::NotificationStreamOpened {
            remote: peer_1.clone(),
            protocol: protocol.clone(),
            role: ObservedRole::Authority,
        };

        event_sender.start_send(msg).expect("Event stream is unbounded");

        block_on(poll_fn(|cx| {
            let mut sent = false;
            loop {
                if let Poll::Ready(()) = bridge.poll_unpin(cx) {
                    unreachable!("we should have a new network event");
                }

                let o = observer.inner.lock().unwrap();

                if sent {
                    let notifications: &Vec<(Vec<u8>, Vec<u8>)> = &network.inner.lock().unwrap().notifications;
                    assert_eq!(notifications[0], (peer.to_bytes(), b"this rocks".to_vec()));
                    assert_eq!(notifications[1], (peer_1.to_bytes(), b"this rocks".to_vec()));
                    break;
                }

                if let Some(_) = &o.0 {
                    if !sent {
                        comms.lock().unwrap().broadcast(b"this rocks".to_vec());
                        sent = true;
                    }
                }

            }
            Poll::Ready(())
        }));
    }

}
