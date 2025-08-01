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

use cf_chains::{Chain, Ethereum};
use ethers::{prelude::abigen, types::Bloom};
use sp_core::{H160, H256};
use tracing::{info, trace};

use super::super::{
	common::{
		chain_source::ChainClient,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	evm::contract_common::events_at_block,
};
use crate::evm::retry_rpc::EvmRetryRpcApi;
use cf_primitives::EpochIndex;
use futures_core::Future;

abigen!(
	StateChainGateway,
	"$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IStateChainGateway.json"
);

use anyhow::Result;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn state_chain_gateway_witnessing<
		EvmRpcClient: EvmRetryRpcApi + ChainClient + Clone,
		ProcessCall,
		ProcessingFut,
	>(
		self,
		process_call: ProcessCall,
		eth_rpc: EvmRpcClient,
		contract_address: H160,
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
			let mut process_calls = vec![];
			async move {
				for event in events_at_block::<Inner::Chain, StateChainGatewayEvents, _>(
					header,
					contract_address,
					&eth_rpc,
				)
				.await?
				{
					info!("Handling event: {event}");
					let call: state_chain_runtime::RuntimeCall = match event.event_parameters {
						StateChainGatewayEvents::FundedFilter(FundedFilter {
							node_id: account_id,
							amount,
							funder,
						}) => pallet_cf_funding::Call::funded {
							account_id: account_id.into(),
							amount: amount.try_into().expect("Funded amount should fit in u128"),
							funder,
							tx_hash: event.tx_hash.into(),
						}
						.into(),
						StateChainGatewayEvents::RedemptionExecutedFilter(
							RedemptionExecutedFilter { node_id: account_id, amount },
						) => pallet_cf_funding::Call::redeemed {
							account_id: account_id.into(),
							redeemed_amount: amount
								.try_into()
								.expect("Redemption amount should fit in u128"),
							tx_hash: event.tx_hash.to_fixed_bytes(),
						}
						.into(),
						StateChainGatewayEvents::RedemptionExpiredFilter(
							RedemptionExpiredFilter { node_id: account_id, amount: _ },
						) => pallet_cf_funding::Call::redemption_expired {
							account_id: account_id.into(),
							block_number: header.index,
						}
						.into(),
						_ => {
							trace!("Ignoring unused event: {event}");
							continue
						},
					};
					process_calls.push(process_call(call, epoch.index));
				}
				futures::future::join_all(process_calls).await;

				Result::Ok(header.data)
			}
		})
	}
}
