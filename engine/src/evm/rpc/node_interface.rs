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

use alloy_eips::BlockId;
use alloy_primitives::Bytes;
use sp_core::{H160, U256};

use anyhow::Result;

use super::{EvmRpcClient, EvmRpcSigningClient};
use crate::evm::conversions::{IntoAlloy, IntoSpCore};

alloy_sol_types::sol!(
	#[sol(all_derives)]
	NodeInterface,
	concat!(env!("CF_ARB_CONTRACT_ABI_ROOT"), "/INodeInterface.json")
);

// It's not actually deployed on-chain but it's accessible via RPC's. See:
// https://docs.arbitrum.io/build-decentralized-apps/nodeinterface/reference
const NODE_INTERFACE_ADDRESS: H160 =
	H160([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xC8]);

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
		let NodeInterface::gasEstimateComponentsReturn {
			gasEstimate,
			gasEstimateForL1,
			baseFee,
			l1BaseFeeEstimate,
		} = self
			.call_contract(
				NODE_INTERFACE_ADDRESS,
				NodeInterface::gasEstimateComponentsCall {
					to: destination_address.into_alloy(),
					contractCreation: contract_creation,
					data: tx_data,
				},
				BlockId::latest(),
			)
			.await?;
		Ok((
			gasEstimate,
			gasEstimateForL1,
			baseFee.into_sp_core(),
			l1BaseFeeEstimate.into_sp_core(),
		))
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
