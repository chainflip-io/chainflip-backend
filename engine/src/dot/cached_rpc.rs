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

use crate::{
	caching_request::CachingRequest,
	dot::{
		retry_rpc::{DotRetryRpcClient, DotRetrySigningRpcApi},
		PolkadotHash, PolkadotHeader,
	},
};
use cf_chains::dot::{PolkadotAccountId, RuntimeVersion};
use cf_primitives::{chains::assets::hub::Asset as HubAsset, PolkadotBlockNumber};
use cf_utilities::task_scope::Scope;
use subxt::{backend::legacy::rpc_methods::Bytes, events::Events, PolkadotConfig};
use tokio::sync::mpsc;

type RawPolkadotAccountId = [u8; 32];

#[async_trait::async_trait]
pub trait DotRetryRpcApiWithResult: Clone {
	async fn block_hash(
		&self,
		block_number: PolkadotBlockNumber,
	) -> anyhow::Result<Option<PolkadotHash>>;

	async fn finalized_head(&self) -> anyhow::Result<PolkadotHash>;

	async fn header(&self, block_hash: PolkadotHash) -> anyhow::Result<Option<PolkadotHeader>>;

	async fn extrinsics(&self, block_hash: PolkadotHash) -> anyhow::Result<Vec<Bytes>>;

	async fn events(
		&self,
		block_hash: PolkadotHash,
		parent_hash: PolkadotHash,
	) -> anyhow::Result<Option<Events<PolkadotConfig>>>;

	async fn runtime_version(
		&self,
		block_hash: Option<PolkadotHash>,
	) -> anyhow::Result<RuntimeVersion>;

	async fn liquid_account_balance(
		&self,
		account_id: PolkadotAccountId,
		asset: HubAsset,
		block_hash: PolkadotHash,
	) -> anyhow::Result<u128>;
}

#[derive(Clone)]
pub struct DotCachingClient {
	retry_client: DotRetryRpcClient,
	block_hash: CachingRequest<PolkadotBlockNumber, Option<PolkadotHash>, DotRetryRpcClient>,
	finalized_head: CachingRequest<(), PolkadotHash, DotRetryRpcClient>,
	header: CachingRequest<PolkadotHash, Option<PolkadotHeader>, DotRetryRpcClient>,
	extrinsics: CachingRequest<PolkadotHash, Vec<Bytes>, DotRetryRpcClient>,
	events: CachingRequest<
		(PolkadotHash, PolkadotHash),
		Option<Events<PolkadotConfig>>,
		DotRetryRpcClient,
	>,
	runtime_version: CachingRequest<Option<PolkadotHash>, RuntimeVersion, DotRetryRpcClient>,
	liquid_account_balance:
		CachingRequest<(RawPolkadotAccountId, HubAsset, PolkadotHash), u128, DotRetryRpcClient>,

	pub cache_invalidation_senders: Vec<mpsc::Sender<()>>,
}

impl DotCachingClient {
	pub(crate) fn new(scope: &Scope<'_, anyhow::Error>, client: DotRetryRpcClient) -> Self {
		let (block_hash, block_hash_cache) = CachingRequest::<
			PolkadotBlockNumber,
			Option<PolkadotHash>,
			DotRetryRpcClient,
		>::new(scope, client.clone());
		let (finalized_head, finalized_head_cache) =
			CachingRequest::<(), PolkadotHash, DotRetryRpcClient>::new(scope, client.clone());
		let (header, header_cache) = CachingRequest::<
			PolkadotHash,
			Option<PolkadotHeader>,
			DotRetryRpcClient,
		>::new(scope, client.clone());
		let (extrinsics, extrinsics_cache) = CachingRequest::<
			PolkadotHash,
			Vec<Bytes>,
			DotRetryRpcClient,
		>::new(scope, client.clone());
		let (events, events_cache) = CachingRequest::<
			(PolkadotHash, PolkadotHash),
			Option<Events<PolkadotConfig>>,
			DotRetryRpcClient,
		>::new(scope, client.clone());
		let (runtime_version, runtime_version_cache) = CachingRequest::<
			Option<PolkadotHash>,
			RuntimeVersion,
			DotRetryRpcClient,
		>::new(scope, client.clone());
		let (liquid_account_balance, liquid_account_balance_cache) = CachingRequest::<
			(RawPolkadotAccountId, HubAsset, PolkadotHash),
			u128,
			DotRetryRpcClient,
		>::new(scope, client.clone());

		Self {
			retry_client: client,
			block_hash,
			finalized_head,
			header,
			extrinsics,
			events,
			runtime_version,
			liquid_account_balance,
			cache_invalidation_senders: vec![
				block_hash_cache,
				finalized_head_cache,
				header_cache,
				extrinsics_cache,
				events_cache,
				runtime_version_cache,
				liquid_account_balance_cache,
			],
		}
	}
}

#[async_trait::async_trait]
impl DotRetrySigningRpcApi for DotCachingClient {
	async fn submit_raw_encoded_extrinsic(
		&self,
		encoded_bytes: Vec<u8>,
	) -> anyhow::Result<PolkadotHash> {
		self.retry_client.submit_raw_encoded_extrinsic(encoded_bytes).await
	}
}

#[async_trait::async_trait]
impl DotRetryRpcApiWithResult for DotCachingClient {
	async fn block_hash(
		&self,
		block_number: PolkadotBlockNumber,
	) -> anyhow::Result<Option<PolkadotHash>> {
		self.block_hash
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move {
						DotRetryRpcApiWithResult::block_hash(&client, block_number).await
					})
				}),
				block_number,
			)
			.await
	}

	async fn finalized_head(&self) -> anyhow::Result<PolkadotHash> {
		self.finalized_head
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move { DotRetryRpcApiWithResult::finalized_head(&client).await })
				}),
				(),
			)
			.await
	}

	async fn header(&self, block_hash: PolkadotHash) -> anyhow::Result<Option<PolkadotHeader>> {
		self.header
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(
						async move { DotRetryRpcApiWithResult::header(&client, block_hash).await },
					)
				}),
				block_hash,
			)
			.await
	}

	async fn extrinsics(&self, block_hash: PolkadotHash) -> anyhow::Result<Vec<Bytes>> {
		self.extrinsics
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move {
						DotRetryRpcApiWithResult::extrinsics(&client, block_hash).await
					})
				}),
				block_hash,
			)
			.await
	}

	async fn events(
		&self,
		block_hash: PolkadotHash,
		parent_hash: PolkadotHash,
	) -> anyhow::Result<Option<Events<PolkadotConfig>>> {
		self.events
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move {
						DotRetryRpcApiWithResult::events(&client, block_hash, parent_hash).await
					})
				}),
				(block_hash, parent_hash),
			)
			.await
	}

	async fn runtime_version(
		&self,
		block_hash: Option<PolkadotHash>,
	) -> anyhow::Result<RuntimeVersion> {
		self.runtime_version
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move {
						DotRetryRpcApiWithResult::runtime_version(&client, block_hash).await
					})
				}),
				block_hash,
			)
			.await
	}

	async fn liquid_account_balance(
		&self,
		account_id: PolkadotAccountId,
		asset: HubAsset,
		block_hash: PolkadotHash,
	) -> anyhow::Result<u128> {
		let request_key = (account_id.0, asset, block_hash);
		self.liquid_account_balance
			.get_or_fetch(
				Box::pin(move |client| {
					let account_id = account_id.clone();
					Box::pin(async move {
						DotRetryRpcApiWithResult::liquid_account_balance(
							&client, account_id, asset, block_hash,
						)
						.await
					})
				}),
				request_key,
			)
			.await
	}
}
