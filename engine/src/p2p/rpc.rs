use futures::{Future};
use jsonrpc_core_client::{RpcError, RpcChannel, TypedClient, TypedSubscriptionStream};
use cf_p2p_rpc::RpcEvent;
use crate::p2p::{P2PNetworkClient, P2PMessage, P2PNetworkClientError, StatusCode};
use tokio::sync::mpsc::UnboundedReceiver;
use jsonrpc_core_client::transports::http::connect;
use tokio_compat_02::FutureExt;
use async_trait::async_trait;
use std::str;
use crate::p2p::ValidatorId;

pub trait Base58 {
	fn to_base58(&self) -> String;
}

impl Base58 for () {
	fn to_base58(&self) -> String {
		"".to_string()
	}
}

struct GlueClient<'a> {
	url: &'a str,
}

#[async_trait]
impl<'a, NodeId: Base58 + Send + Sync> P2PNetworkClient<NodeId> for GlueClient<'a> {
	async fn broadcast(&self, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError> {
		let msg = str::from_utf8(data).map_err(|_| P2PNetworkClientError::Format)?;
		let client: P2PClient = FutureExt::compat(connect(self.url))
			.await
			.map_err(|_| P2PNetworkClientError::Rpc)?;
		client.broadcast(msg.to_string()).await.map_err(|_| P2PNetworkClientError::Rpc)
	}

	async fn send(&self, to: &NodeId, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError> {
		let msg = str::from_utf8(data).map_err(|_| P2PNetworkClientError::Format)?;
		let client: P2PClient = FutureExt::compat(connect(self.url))
			.await
			.map_err(|_| P2PNetworkClientError::Rpc)?;
		client.send(to.to_base58(),msg.to_string()).await.map_err(|_| P2PNetworkClientError::Rpc)
	}

	async fn take_receiver(&mut self) -> Option<UnboundedReceiver<P2PMessage>> {
		todo!()
	}
}

#[derive(Clone)]
struct P2PClient {
	inner: TypedClient,
}

impl From<RpcChannel> for P2PClient {
	fn from(channel: RpcChannel) -> Self {
		P2PClient::new(channel.into())
	}
}

impl P2PClient {
	/// Creates a new `P2PClient`.
	pub fn new(sender: RpcChannel) -> Self {
		P2PClient {
			inner: sender.into(),
		}
	}
	/// Send a message to peer id returning a HTTP status code
	pub fn send(
		&self,
		peer_id: String,
		message: String,
	) -> impl Future<Output = Result<u64, RpcError>> {
		let args = (peer_id, message);
		self.inner.call_method("p2p_send", "u64", args)
	}

	/// Broadcast a message to the p2p network returning a HTTP status code
	pub fn broadcast(&self, message: String) -> impl Future<Output = Result<u64, RpcError>> {
		let args = (message,);
		self.inner.call_method("p2p_broadcast", "u64", args)
	}
	// Subscribe to receive notifications
	// pub fn subscribe_notifications(
	// 	&self,
	// ) -> impl Future<Output = Result<TypedSubscriptionStream<RpcEvent>, RpcError>>
	// {
	// 	let args_tuple = ();
	// 	self.inner.subscribe(
	// 		"cf_p2p_subscribeNotifications",
	// 		args_tuple,
	// 		"cf_p2p_notifications",
	// 		"cf_p2p_unsubscribeNotifications",
	// 		"RpcEvent",
	// 	)
	// }
}

#[cfg(test)]
mod tests {
	use super::*;
	use jsonrpc_core_client::transports::http::connect;
	use tokio_compat_02::FutureExt;

	#[tokio::test]
	async fn should_work() {
		let client: P2PClient = FutureExt::compat(connect(&"http://localhost:9933")).await.unwrap();
		let result = client.send("123".to_string(), "hello".to_string()).await;
		assert_eq!(result.unwrap(), 200);
	}
}
