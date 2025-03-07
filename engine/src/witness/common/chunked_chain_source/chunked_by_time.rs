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

pub mod builder;
pub mod chain_tracking;

use futures_util::StreamExt;

use crate::witness::common::{
	chain_source::{aliases, BoxChainStream, ChainClient, ChainStream},
	epoch_source::{Epoch, EpochSource},
	BoxActiveAndFuture, ExternalChain, ExternalChainSource,
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByTime: Sized + Send + Sync {
	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index>;

	type Parameters: Send;

	async fn stream(&self, parameters: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>>;
}

pub type Item<'a, T> = (
	Epoch<(), ()>,
	BoxChainStream<
		'a,
		<T as ChunkedByTime>::Index,
		<T as ChunkedByTime>::Hash,
		<T as ChunkedByTime>::Data,
	>,
	<T as ChunkedByTime>::Client,
);

#[async_trait::async_trait]
impl<T: ChunkedChainSource<Info = (), HistoricInfo = ()>> ChunkedByTime for T {
	type Index = T::Index;
	type Hash = T::Hash;
	type Data = T::Data;

	type Client = T::Client;

	type Chain = T::Chain;

	type Parameters = T::Parameters;

	async fn stream(&self, parameters: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>> {
		<Self as ChunkedChainSource>::stream(self, parameters).await
	}
}

#[derive(Clone)]
pub struct ChunkByTime<TChainSource> {
	chain_source: TChainSource,
}

impl<TChainSource> ChunkByTime<TChainSource> {
	pub fn new(chain_source: TChainSource) -> Self {
		Self { chain_source }
	}
}
#[async_trait::async_trait]
impl<TChainSource: ExternalChainSource> ChunkedByTime for ChunkByTime<TChainSource> {
	type Index = TChainSource::Index;
	type Hash = TChainSource::Hash;
	type Data = TChainSource::Data;

	type Client = TChainSource::Client;

	type Chain = TChainSource::Chain;

	type Parameters = EpochSource<(), ()>;

	async fn stream(&self, epochs: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>> {
		epochs
			.into_stream()
			.await
			.then(move |epoch| async move {
				let (stream, client) = self.chain_source.stream_and_client().await;
				let historic_signal = epoch.historic_signal.clone();
				(epoch, stream.take_until(historic_signal.wait()).into_box(), client)
			})
			.await
			.into_box()
	}
}
