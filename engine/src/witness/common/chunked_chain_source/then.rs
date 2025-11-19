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

use futures_core::Future;
use futures_util::StreamExt;

use crate::witness::common::{
	chain_source::{aliases, ChainClient, ChainStream, Header},
	epoch_source::Epoch,
	BoxActiveAndFuture,
};

use super::ChunkedChainSource;

pub struct Then<Inner, F> {
	inner: Inner,
	f: F,
}
impl<Inner, F> Then<Inner, F> {
	pub fn new(inner: Inner, f: F) -> Self {
		Self { inner, f }
	}
}
#[async_trait::async_trait]
impl<Inner, Output, Fut, F> ChunkedChainSource for Then<Inner, F>
where
	Output: aliases::Data,
	Inner: ChunkedChainSource,
	Fut: Future<Output = Output> + Send,
	F: Fn(
			Epoch<Inner::Info, Inner::HistoricInfo>,
			Header<Inner::Index, Inner::Hash, Inner::Data>,
		) -> Fut
		+ Send
		+ Sync
		+ Clone,
{
	type Info = Inner::Info;
	type HistoricInfo = Inner::HistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Output;

	type Client = ThenClient<Inner, F>;

	type Chain = Inner::Chain;

	type Parameters = Inner::Parameters;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> BoxActiveAndFuture<'_, super::Item<'_, Self, Self::Info, Self::HistoricInfo>> {
		self.inner
			.stream(parameters)
			.await
			.then(move |(epoch, chain_stream, chain_client)| async move {
				(
					epoch.clone(),
					chain_stream
						.then({
							let epoch = epoch.clone();
							move |header| {
								let epoch = epoch.clone();
								header.then_data(move |header| async move {
									(self.f)(epoch, header).await
								})
							}
						})
						.into_box(),
					ThenClient::new(chain_client, self.f.clone(), epoch),
				)
			})
			.await
			.into_box()
	}
}

pub struct ThenClient<Inner: ChunkedChainSource, F> {
	inner_client: Inner::Client,
	f: F,
	epoch: Epoch<Inner::Info, Inner::HistoricInfo>,
}

impl<Inner: ChunkedChainSource, F: Clone> Clone for ThenClient<Inner, F> {
	fn clone(&self) -> Self {
		Self {
			inner_client: self.inner_client.clone(),
			f: self.f.clone(),
			epoch: self.epoch.clone(),
		}
	}
}

impl<Inner: ChunkedChainSource, F> ThenClient<Inner, F> {
	pub fn new(
		inner_client: Inner::Client,
		f: F,
		epoch: Epoch<Inner::Info, Inner::HistoricInfo>,
	) -> Self {
		Self { inner_client, f, epoch }
	}
}
#[async_trait::async_trait]
impl<
		Output: aliases::Data,
		Inner: ChunkedChainSource,
		Fut: Future<Output = Output> + Send,
		F: Fn(
				Epoch<Inner::Info, Inner::HistoricInfo>,
				Header<Inner::Index, Inner::Hash, Inner::Data>,
			) -> Fut
			+ Send
			+ Sync
			+ Clone,
	> ChainClient for ThenClient<Inner, F>
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Output;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.inner_client
			.header_at_index(index)
			.await
			.then_data(move |header| (self.f)(self.epoch.clone(), header))
			.await
	}
}
