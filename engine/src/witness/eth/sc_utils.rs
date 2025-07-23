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

use std::collections::HashMap;

use cf_chains::{Chain, Ethereum};
use ethers::{prelude::abigen, types::Bloom};
use sp_core::{H160, H256};
use tracing::{info, warn};

use super::super::{
	common::{
		chain_source::ChainClient,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	evm::contract_common::events_at_block,
};
use crate::evm::retry_rpc::EvmRetryRpcApi;
use cf_chains::evm::ToAccountId32;
use cf_primitives::{Asset, EpochIndex};
use futures_core::Future;
use pallet_cf_funding::{EthereumDeposit, EthereumDepositAndSCCall};

abigen!(ScUtils, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IScUtils.json");

use anyhow::Result;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn sc_utils_witnessing<
		EvmRpcClient: EvmRetryRpcApi + ChainClient + Clone,
		ProcessCall,
		ProcessingFut,
	>(
		self,
		process_call: ProcessCall,
		eth_rpc: EvmRpcClient,
		contract_address: H160,
		supported_assets: HashMap<H160, Asset>,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner: ChunkedByVault<Index = u64, Hash = H256, Data = Bloom, Chain = Ethereum>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
	{
		self.then::<Result<Bloom>, _, _>(move |epoch, header| {
			assert!(<Inner::Chain as Chain>::is_block_witness_root(header.index));

			let process_call = process_call.clone();
			let eth_rpc = eth_rpc.clone();
			let supported_assets = supported_assets.clone();
			async move {
				for event in events_at_block::<Inner::Chain, ScUtilsEvents, _>(
					header,
					contract_address,
					&eth_rpc,
				)
				.await?
				{
					info!("Handling event: {event}");
					process_call(
						match event.event_parameters {
							ScUtilsEvents::DepositToScGatewayAndScCallFilter(
								DepositToScGatewayAndScCallFilter {
									sender,    // eth_address to attribute the FLIP to
									signer: _, // `tx.origin``. Not to be used for now
									amount,    // FLIP amount deposited
									sc_call,
								},
							) => pallet_cf_funding::Call::execute_sc_call {
								deposit_and_call: EthereumDepositAndSCCall {
									deposit: EthereumDeposit::FlipToSCGateway {
										amount: amount.try_into().unwrap(),
									},
									call: sc_call.to_vec(),
								},
								caller: sender,
								// use 0 padded ethereum address as account_id which the flip funds
								// are associated with on SC
								caller_account_id: sender.into_account_id_32(),
								tx_hash: event.tx_hash.to_fixed_bytes(),
							},
							ScUtilsEvents::DepositToVaultAndScCallFilter(
								DepositToVaultAndScCallFilter {
									sender,
									signer: _,
									amount,
									token,
									sc_call,
								},
							) => {
								if let Some(asset) = supported_assets.get(&token) {
									pallet_cf_funding::Call::execute_sc_call {
										deposit_and_call: EthereumDepositAndSCCall {
											deposit: EthereumDeposit::Vault {
												asset: (*asset).try_into().unwrap(),
												amount: amount.try_into().unwrap(),
											},
											call: sc_call.to_vec(),
										},
										caller: sender,
										// use 0 padded ethereum address as account_id which the
										// flip funds are associated with on SC
										caller_account_id: sender.into_account_id_32(),
										tx_hash: event.tx_hash.to_fixed_bytes(),
									}
								} else {
									warn!("unsupported asset deposited: {token}. Ignoring deposit");
									continue;
								}
							},

							ScUtilsEvents::DepositAndScCallFilter(DepositAndScCallFilter {
								sender,
								signer: _,
								amount,
								token,
								to,
								sc_call,
							}) => {
								if let Some(asset) = supported_assets.get(&token) {
									pallet_cf_funding::Call::execute_sc_call {
										deposit_and_call: EthereumDepositAndSCCall {
											deposit: EthereumDeposit::Transfer {
												asset: (*asset).try_into().unwrap(),
												amount: amount.try_into().unwrap(),
												destination: to,
											},
											call: sc_call.to_vec(),
										},
										caller: sender,
										// use 0 padded ethereum address as account_id which the
										// flip funds are associated with on SC
										caller_account_id: sender.into_account_id_32(),
										tx_hash: event.tx_hash.to_fixed_bytes(),
									}
								} else {
									warn!("unsupported asset deposited: {token}. Ignoring deposit");
									continue;
								}
							},

							ScUtilsEvents::CallScFilter(CallScFilter {
								sender,
								signer: _,
								sc_call,
							}) => pallet_cf_funding::Call::execute_sc_call {
								deposit_and_call: EthereumDepositAndSCCall {
									deposit: EthereumDeposit::NoDeposit,
									call: sc_call.to_vec(),
								},
								caller: sender,
								// use 0 padded ethereum address as account_id which the
								// flip funds are associated with on SC
								caller_account_id: sender.into_account_id_32(),
								tx_hash: event.tx_hash.to_fixed_bytes(),
							},
						}
						.into(),
						epoch.index,
					)
					.await;
				}

				Result::Ok(header.data)
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use codec::Encode;
	use frame_support::sp_runtime::AccountId32;
	use pallet_cf_funding::{DelegationApi, EthereumSCApi};
	use state_chain_runtime::Runtime;

	#[test]
	fn test_sc_call_encode() {
		let sc_call_delegate = EthereumSCApi::Delegation(DelegationApi::<Runtime>::Delegate {
			delegator: [0xf5; 20].into(),
			operator: AccountId32::new([0xF4; 32]),
		})
		.encode();
		assert_eq!(
			sc_call_delegate,
			hex::decode("0000f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4")
				.unwrap()
		);
	}
}
