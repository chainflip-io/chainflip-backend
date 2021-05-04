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

pub trait NetworkT : Clone {
    fn write_notification(&self, who: PeerId, protocol: Cow<'static, str>, message: Vec<u8>);
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>>;
}

impl<B: BlockT, H: ExHashT> NetworkT for Arc<NetworkService<B, H>> {
    fn write_notification(&self, target: PeerId, protocol: Cow<'static, str>, message: Vec<u8>) {
        NetworkService::write_notification(self, target, protocol, message)
    }
    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        Box::pin(NetworkService::event_stream(self, "network-chainflip"))
    }
}

pub trait Observer {
    fn new_peer(&self, peer_id: &PeerId);
    fn disconnected(&self, peer_id: &PeerId);
    fn received(&self, peer_id: &PeerId, messages: Message);
}

pub struct DeafObserver;
impl Observer for DeafObserver {
    fn new_peer(&self, _peer_id: &PeerId) {}
    fn disconnected(&self, _peer_id: &PeerId) {}
    fn received(&self, _peer_id: &PeerId, _messages: Message) {}
}

struct StateMachine<O: Observer, N: NetworkT> {
    observer: Arc<O>,
    network: N,
    peers: HashMap<PeerId, ()>,
    protocol: Cow<'static, str>,
}

impl<O, N> StateMachine<O, N>
    where
        O: Observer,
        N: NetworkT,
{
    pub fn new(observer: Arc<O>, network: N, protocol: Cow<'static, str>) -> Self {
        StateMachine {
            observer,
            network,
            peers: HashMap::new(),
            protocol: protocol.into(),
        }
    }

    pub fn new_peer(&mut self, peer_id: &PeerId) {
        self.peers.insert(peer_id.clone(), ());
        self.observer.new_peer(peer_id);
    }

    pub fn disconnected(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
        self.observer.disconnected(peer_id);
    }

    pub fn received(&self, peer_id: &PeerId, messages: Vec<Message>) {
        for message in messages {
            self.observer.received(peer_id, message);
        }
    }

    pub fn send_message(&mut self, peer_id: PeerId, message: Message) {
        if self.peers.contains_key(&peer_id) {
            self.network.write_notification(peer_id, self.protocol.clone(), message);
        }
    }

    pub fn broadcast(&mut self, message: Message) {
        for peer_id in self.peers.keys() {
            self.network.write_notification(*peer_id, self.protocol.clone(), message.clone());
        }
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

pub struct NetworkBridge<O: Observer, N: NetworkT> {
    network: N,
    state_machine: StateMachine<O, N>,
    network_event_stream: Pin<Box<dyn Stream<Item = Event> + Send>>,
    protocol: Cow<'static, str>,
    worker: OutgoingMessagesWorker,
    sender: UnboundedSender<(Vec<PeerId>, Message)>,
    communication: Arc<Mutex<Interface>>,
}

impl<O, N> NetworkBridge<O, N>
    where
        O: Observer,
        N: NetworkT,
{
    pub fn new(observer: Arc<O>, network: N, protocol: Cow<'static, str>) -> (Self, Arc<Mutex<Interface>>) {
        let state_machine = StateMachine::new(observer, network.clone(), protocol.clone());
        let network_event_stream = Box::pin(network.event_stream());
        let (worker, sender) = OutgoingMessagesWorker::new();
        let communication = Arc::new(Mutex::new(Interface(sender.clone())));
        (NetworkBridge {
            network: network.clone(),
            state_machine,
            network_event_stream,
            protocol: protocol.clone(),
            worker,
            sender: sender.clone(),
            communication: communication.clone(),
        }, communication.clone())
    }
}

pub trait Communication {
    fn send_message(&mut self, peer_id: &PeerId, data: Message);
    fn broadcast(&self, data: Message);
}

pub struct Interface(UnboundedSender<(Vec<PeerId>, Message)>);

impl Communication for Interface {
    fn send_message(&mut self, peer_id: &PeerId, data: Message) {
        if let Err(e) = self.0.unbounded_send((vec![*peer_id], data)) {
            debug!("Failed to push message to channel {:?}", e);
        }
    }

    fn broadcast(&self, data: Message) {
        if let Err(e) = self.0.unbounded_send((vec![], data)) {
            debug!("Failed to broadcast message to channel {:?}", e);
        }
    }
}

impl<O, N> Unpin for NetworkBridge<O, N>
    where
        O: Observer,
        N: NetworkT, {}


impl<O, N> Future for NetworkBridge<O, N>
    where
        O: Observer,
        N: NetworkT,
{
    type Output = ();

    // TODO return Result for poll
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

    impl NetworkT for TestNetwork {
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

    impl Observer for TestObserver {
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
        let (mut bridge, comms) = NetworkBridge::new(
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
                        comms.lock().unwrap().send_message(peer, b"this rocks".to_vec());
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
