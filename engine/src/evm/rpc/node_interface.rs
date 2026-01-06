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
use std::str::FromStr;

use anyhow::{Ok, Result};

use super::{EvmRpcClient, EvmRpcSigningClient};

abigen!(NodeInterface, "$CF_ARB_CONTRACT_ABI_ROOT/INodeInterface.json");

// It's not actually deployed on-chain but it's accessible via RPC's. See:
// https://docs.arbitrum.io/build-decentralized-apps/nodeinterface/reference
const NODE_INTERFACE_ADDRESS: &str = "0x00000000000000000000000000000000000000C8";

#[async_trait::async_trait]
pub trait NodeInterfaceRpcApi {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> Result<(u64, u64, U256, U256)>;
}

#[async_trait::async_trait]
impl NodeInterfaceRpcApi for EvmRpcClient {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> Result<(u64, u64, U256, U256)> {
		Ok(NodeInterface::new(
			H160::from_str(NODE_INTERFACE_ADDRESS).unwrap(),
			self.provider.clone(),
		)
		.gas_estimate_components(destination_address, contract_creation, tx_data)
		.call()
		.await?)
	}
}

#[async_trait::async_trait]
impl NodeInterfaceRpcApi for EvmRpcSigningClient {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> Result<(u64, u64, U256, U256)> {
		self.rpc_client
			.gas_estimate_components(destination_address, contract_creation, tx_data)
			.await
	}
}
