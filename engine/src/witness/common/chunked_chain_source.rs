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

pub mod and_then;
pub mod chunked_by_time;
pub mod chunked_by_vault;
pub mod latest_then;
pub mod logging;
pub mod then;

use super::{
	chain_source::{aliases, BoxChainStream, ChainClient},
	epoch_source::Epoch,
	BoxActiveAndFuture, ExternalChain,
};

#[async_trait::async_trait]
pub trait ChunkedChainSource: Sized + Send + Sync {
	type Info: Clone + Send + Sync + 'static;
	type HistoricInfo: Clone + Send + Sync + 'static;

	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index>;

	type Parameters: Send;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> BoxActiveAndFuture<'_, Item<'_, Self, Self::Info, Self::HistoricInfo>>;
}

pub type Item<'a, T, Info, HistoricInfo> = (
	Epoch<Info, HistoricInfo>,
	BoxChainStream<
		'a,
		<T as ChunkedChainSource>::Index,
		<T as ChunkedChainSource>::Hash,
		<T as ChunkedChainSource>::Data,
	>,
	<T as ChunkedChainSource>::Client,
);
