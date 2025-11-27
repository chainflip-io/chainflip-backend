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

use super::{aliases, BoxChainStream, ChainSource, ChainStream, Header};

use futures::Future;
use futures_util::StreamExt;

use crate::witness::common::{chain_source::ChainClient, ExternalChainSource};

#[derive(Clone)]
pub struct Then<InnerSource, F> {
	inner_source: InnerSource,
	f: F,
}

impl<InnerSource, F> Then<InnerSource, F> {
	pub fn new(inner_source: InnerSource, f: F) -> Self {
		Self { inner_source, f }
	}
}

#[async_trait::async_trait]
impl<
		Output: aliases::Data,
		InnerSource: ChainSource,
		Fut: Future<Output = Output> + Send,
		F: Fn(Header<InnerSource::Index, InnerSource::Hash, InnerSource::Data>) -> Fut
			+ Send
			+ Sync
			+ Clone,
	> ChainSource for Then<InnerSource, F>
{
	type Index = InnerSource::Index;
	type Hash = InnerSource::Hash;
	type Data = Output;

	type Client = ThenClient<InnerSource::Client, F>;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (inner_stream, inner_client) = self.inner_source.stream_and_client().await;

		(
			inner_stream
				.then(move |header| async move { header.then_data(&self.f).await })
				.into_box(),
			ThenClient::new(inner_client, self.f.clone()),
		)
	}
}

impl<
		Output: aliases::Data,
		InnerSource: ExternalChainSource,
		Fut: Future<Output = Output> + Send,
		F: Fn(Header<InnerSource::Index, InnerSource::Hash, InnerSource::Data>) -> Fut
			+ Send
			+ Sync
			+ Clone,
	> ExternalChainSource for Then<InnerSource, F>
{
	type Chain = InnerSource::Chain;
}

#[derive(Clone)]
pub struct ThenClient<InnerClient, F> {
	inner_client: InnerClient,
	f: F,
}

impl<InnerClient, F> ThenClient<InnerClient, F> {
	pub fn new(inner_client: InnerClient, f: F) -> Self {
		Self { inner_client, f }
	}
}

#[async_trait::async_trait]
impl<
		Output: aliases::Data,
		InnerClient: ChainClient,
		Fut: Future<Output = Output> + Send,
		F: Fn(Header<InnerClient::Index, InnerClient::Hash, InnerClient::Data>) -> Fut
			+ Send
			+ Sync
			+ Clone,
	> ChainClient for ThenClient<InnerClient, F>
{
	type Index = InnerClient::Index;
	type Hash = InnerClient::Hash;
	type Data = Output;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.inner_client.header_at_index(index).await.then_data(&self.f).await
	}
}
