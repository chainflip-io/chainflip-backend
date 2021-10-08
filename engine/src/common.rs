use std::ops::{Deref, DerefMut};

struct MutexStateAndPoisonFlag<T> {
    poisoned: bool,
    state: T,
}

pub struct MutexGuard<'a, T> {
    guard: tokio::sync::MutexGuard<'a, MutexStateAndPoisonFlag<T>>,
}
impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard.deref().state
    }
}
impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard.deref_mut().state
    }
}
impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        let guarded = self.guard.deref_mut();
        if !guarded.poisoned && std::thread::panicking() {
            guarded.poisoned = true;
        }
    }
}

/// This mutex implementation will panic when it is locked iff a thread previously panicked while holding it.
/// This ensures potentially broken data cannot be seen by other threads.
pub struct Mutex<T> {
    mutex: tokio::sync::Mutex<MutexStateAndPoisonFlag<T>>,
}
impl<T> Mutex<T> {
    pub fn new(t: T) -> Self {
        Self {
            mutex: tokio::sync::Mutex::new(MutexStateAndPoisonFlag {
                poisoned: false,
                state: t,
            }),
        }
    }
    pub async fn lock<'a>(&'a self) -> MutexGuard<'a, T> {
        let guard = self.mutex.lock().await;

        if guard.deref().poisoned {
            panic!("Another thread panicked while holding this lock");
        } else {
            MutexGuard { guard }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    #[should_panic]
    async fn mutex_panics_if_poisoned() {
        let mutex = Arc::new(Mutex::new(0));
        {
            let mutex_clone = mutex.clone();
            tokio::spawn(async move {
                let _inner = mutex_clone.lock().await;
                panic!();
            })
            .await
            .unwrap_err();
        }
        mutex.lock().await;
    }

    #[tokio::test]
    async fn mutex_doesnt_panic_if_not_poisoned() {
        let mutex = Arc::new(Mutex::new(0));
        {
            let mutex_clone = mutex.clone();
            tokio::spawn(async move {
                let _inner = mutex_clone.lock().await;
            })
            .await
            .unwrap();
        }
        mutex.lock().await;
    }
}

pub mod alt_jsonrpc_connect {
    use failure::Error;
    use futures::compat::Future01CompatExt;
    use jsonrpc_core::futures::{Async, AsyncSink, Future, Sink, Stream};
    use jsonrpc_core_client::{transports::duplex, RpcChannel, RpcError};
    use std::collections::VecDeque;
    use websocket::{ClientBuilder, OwnedMessage};

    /////////////////////////////////////
    /// This code was copied from jsonrpc_client_transports 15.1.0 src/transports/ws.rs
    /// The only change was to apply compat() to the rpc_client future before passing it to the tokio::spawn() call

    /// Connect to a JSON-RPC websocket server.
    ///
    /// Uses an unbuffered channel to queue outgoing rpc messages.
    pub fn connect<T>(url: &url::Url) -> impl Future<Item = T, Error = RpcError>
    where
        T: From<RpcChannel>,
    {
        let client_builder = ClientBuilder::from_url(url);
        do_connect(client_builder)
    }

    fn do_connect<T>(client_builder: ClientBuilder) -> impl Future<Item = T, Error = RpcError>
    where
        T: From<RpcChannel>,
    {
        client_builder
            .async_connect(None)
            .map(|(client, _)| {
                let (sink, stream) = client.split();
                let (sink, stream) = WebsocketClient::new(sink, stream).split();
                let (rpc_client, sender) = duplex(sink, stream);
                let rpc_client = rpc_client.map_err(|error| eprintln!("{:?}", error));
                tokio::spawn(rpc_client.compat());
                sender.into()
            })
            .map_err(|error| RpcError::Other(error.into()))
    }

    struct WebsocketClient<TSink, TStream> {
        sink: TSink,
        stream: TStream,
        queue: VecDeque<OwnedMessage>,
    }

    impl<TSink, TStream, TError> WebsocketClient<TSink, TStream>
    where
        TSink: Sink<SinkItem = OwnedMessage, SinkError = TError>,
        TStream: Stream<Item = OwnedMessage, Error = TError>,
        TError: Into<Error>,
    {
        fn new(sink: TSink, stream: TStream) -> Self {
            Self {
                sink,
                stream,
                queue: VecDeque::new(),
            }
        }
    }

    impl<TSink, TStream, TError> Sink for WebsocketClient<TSink, TStream>
    where
        TSink: Sink<SinkItem = OwnedMessage, SinkError = TError>,
        TStream: Stream<Item = OwnedMessage, Error = TError>,
        TError: Into<Error>,
    {
        type SinkItem = String;
        type SinkError = RpcError;

        fn start_send(
            &mut self,
            request: Self::SinkItem,
        ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
            self.queue.push_back(OwnedMessage::Text(request));
            Ok(AsyncSink::Ready)
        }

        fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
            loop {
                match self.queue.pop_front() {
                    Some(request) => match self.sink.start_send(request) {
                        Ok(AsyncSink::Ready) => continue,
                        Ok(AsyncSink::NotReady(request)) => {
                            self.queue.push_front(request);
                            break;
                        }
                        Err(error) => return Err(RpcError::Other(error.into())),
                    },
                    None => break,
                }
            }
            self.sink
                .poll_complete()
                .map_err(|error| RpcError::Other(error.into()))
        }
    }

    impl<TSink, TStream, TError> Stream for WebsocketClient<TSink, TStream>
    where
        TSink: Sink<SinkItem = OwnedMessage, SinkError = TError>,
        TStream: Stream<Item = OwnedMessage, Error = TError>,
        TError: Into<Error>,
    {
        type Item = String;
        type Error = RpcError;

        fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
            loop {
                match self.stream.poll() {
                    Ok(Async::Ready(Some(message))) => match message {
                        OwnedMessage::Text(data) => return Ok(Async::Ready(Some(data))),
                        OwnedMessage::Binary(_) => (),
                        OwnedMessage::Ping(p) => self.queue.push_front(OwnedMessage::Pong(p)),
                        OwnedMessage::Pong(_) => {}
                        OwnedMessage::Close(c) => self.queue.push_front(OwnedMessage::Close(c)),
                    },
                    Ok(Async::Ready(None)) => {
                        // TODO try to reconnect (#411).
                        return Ok(Async::Ready(None));
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(error) => return Err(RpcError::Other(error.into())),
                }
            }
        }
    }
}
