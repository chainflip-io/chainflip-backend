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

use async_trait::async_trait;
use cf_chains::dot::{PolkadotAccountId, RuntimeVersion};
use cf_primitives::{chains::assets::hub::Asset as HubAsset, PolkadotBlockNumber};
use subxt::{
	backend::legacy::rpc_methods::{BlockDetails, Bytes},
	events::Events,
	PolkadotConfig,
};

use crate::dot::{PolkadotHash, PolkadotHeader};
use anyhow::Result;

/// The trait that defines the stateless / non-subscription requests to Polkadot.
#[async_trait]
pub trait DotRpcApi: Send + Sync {
	async fn block_hash(&self, block_number: PolkadotBlockNumber) -> Result<Option<PolkadotHash>>;

	async fn finalized_head(&self) -> Result<PolkadotHash>;

	async fn header(&self, block_hash: PolkadotHash) -> Result<Option<PolkadotHeader>>;

	async fn block(&self, block_hash: PolkadotHash)
		-> Result<Option<BlockDetails<PolkadotConfig>>>;

	async fn extrinsics(&self, block_hash: PolkadotHash) -> Result<Option<Vec<Bytes>>>;

	async fn events(
		&self,
		block_hash: PolkadotHash,
		parent_hash: PolkadotHash,
	) -> Result<Option<Events<PolkadotConfig>>>;

	async fn runtime_version(&self, at: Option<PolkadotHash>) -> Result<RuntimeVersion>;

	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash>;

	async fn liquid_account_balance(
		&self,
		account_id: PolkadotAccountId,
		asset: HubAsset,
		block_hash: PolkadotHash,
	) -> Result<u128>;
}
