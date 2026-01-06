// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

pub mod cached_rpc;
pub mod retry_rpc;
pub mod rpc;

use anyhow::{Context, Result};

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
pub struct ConscientiousEvmWebsocketBlockHeaderStream {
	stream: Option<
		web3::api::SubscriptionStream<web3::transports::WebSocket, web3::types::BlockHeader>,
	>,
	chain_name: &'static str,
}

impl ConscientiousEvmWebsocketBlockHeaderStream {
	pub async fn new(
		web3: web3::Web3<web3::transports::WebSocket>,
		chain_name: &'static str,
	) -> Result<Self> {
		Ok(Self {
			stream: Some(
				web3.eth_subscribe()
					.subscribe_new_heads()
					.await
					.context("Failed to subscribe to new heads with WS Client")?,
			),
			chain_name,
		})
	}
}

impl Drop for ConscientiousEvmWebsocketBlockHeaderStream {
	fn drop(&mut self) {
		tracing::warn!("Dropping the {} WS connection", self.chain_name);
		self.stream.take().unwrap().unsubscribe().now_or_never();
	}
}

impl Stream for ConscientiousEvmWebsocketBlockHeaderStream {
	type Item = Result<web3::types::BlockHeader, web3::Error>;

	fn poll_next(
		mut self: Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> std::task::Poll<Option<Self::Item>> {
		Pin::new(self.stream.as_mut().unwrap()).poll_next(cx)
	}
}
