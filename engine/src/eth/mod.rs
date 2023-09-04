pub mod event;
pub mod redact_endpoint_secret;
pub mod retry_rpc;
pub mod rpc;

use anyhow::Result;

use futures::FutureExt;

use std::pin::Pin;

use tokio_stream::Stream;

pub fn core_h256(h: web3::types::H256) -> sp_core::H256 {
	h.0.into()
}

pub fn core_h160(h: web3::types::H160) -> sp_core::H160 {
	h.0.into()
}

/// Wraps a web3 crate stream so it unsubscribes when dropped.
pub struct ConscientiousEthWebsocketBlockHeaderStream {
	stream: Option<
		web3::api::SubscriptionStream<web3::transports::WebSocket, web3::types::BlockHeader>,
	>,
}

impl Drop for ConscientiousEthWebsocketBlockHeaderStream {
	fn drop(&mut self) {
		tracing::warn!("Dropping the ETH WS connection");
		self.stream.take().unwrap().unsubscribe().now_or_never();
	}
}

impl Stream for ConscientiousEthWebsocketBlockHeaderStream {
	type Item = Result<web3::types::BlockHeader, web3::Error>;

	fn poll_next(
		mut self: Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> std::task::Poll<Option<Self::Item>> {
		Pin::new(self.stream.as_mut().unwrap()).poll_next(cx)
	}
}
