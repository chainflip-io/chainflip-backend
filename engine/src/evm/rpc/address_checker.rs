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

use ethers::prelude::*;

use anyhow::{Ok, Result};

use super::{EvmRpcClient, EvmRpcSigningClient};

abigen!(AddressChecker, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IAddressChecker.json");

#[async_trait::async_trait]
pub trait AddressCheckerRpcApi {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>>;

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<U256>>;
}

#[async_trait::async_trait]
impl AddressCheckerRpcApi for EvmRpcClient {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>> {
		Ok(AddressChecker::new(contract_address, self.provider.clone())
			.address_states(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
	}

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<U256>> {
		Ok(AddressChecker::new(contract_address, self.provider.clone())
			.native_balances(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
	}
}

#[async_trait::async_trait]
impl AddressCheckerRpcApi for EvmRpcSigningClient {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>> {
		self.rpc_client.address_states(block_hash, contract_address, addresses).await
	}

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<U256>> {
		self.rpc_client.balances(block_hash, contract_address, addresses).await
	}
}
