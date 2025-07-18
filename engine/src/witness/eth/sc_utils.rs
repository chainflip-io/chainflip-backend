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
use cf_chains::evm::ToAccountId32;
use cf_primitives::EpochIndex;
use futures_core::Future;
use pallet_cf_funding::DepositAndSCCallViaEthereum;

abigen!(ScUtils, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IScUtils.json");

use anyhow::Result;

// We could decode the ScCall bytes into a Vec<ScCall> to allow multiple actions into a single
// transaction. The challenge with that is that we'd need to be very careful to make sure that none
// of the possible combinations can be abused in any way.
// Alternatively, the safer approach taken here is to limit explicitly the actions that can be taken
// and allow for particular actions to be batched under the same enum. For example, having the
// Undelegate and the UndelegateAndRedeem under the ScCallViaGateway enum.
// TODO: To discuss if this is  the approach we want to take.

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
			async move {
				for event in events_at_block::<Inner::Chain, ScUtilsEvents, _>(
					header,
					contract_address,
					&eth_rpc,
				)
				.await?
				{
					info!("Handling event: {event}");
					let call: state_chain_runtime::RuntimeCall = match event.event_parameters {
						ScUtilsEvents::DepositToScGatewayAndScCallFilter(
							DepositToScGatewayAndScCallFilter {
								sender,    // eth_address to attribute the FLIP to
								signer: _, // `tx.origin``. Not to be used for now
								amount,    // FLIP amount deposited
								sc_call,
							},
						) => {
							pallet_cf_funding::Call::execute_sc_call {
								sc_call: sc_call.to_vec(),
								deposit_and_call:
									DepositAndSCCallViaEthereum::FlipToSCGatewayAndCall {
										amount: amount.try_into().unwrap(),
										call: None, /* This will be filled on the SC after SC
										             * decodes
										             * the call from the sc_call bytes above */
									},
								caller: sender,
								// use 0 padded ethereum address as account_id which the flip funds
								// are associated with on SC
								caller_account_id: sender.into_account_id_32(),
								tx_hash: event.tx_hash.to_fixed_bytes(),
							}
							.into()
						},
						_ => {
							trace!("Ignoring unused event: {event}");
							continue
						},
					};
					process_call(call, epoch.index).await;
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
	use pallet_cf_funding::AllowedCallsViaSCGateway;
	use state_chain_runtime::Runtime;

	#[test]
	fn test_sc_call_encode() {
		let sc_call_delegate = AllowedCallsViaSCGateway::<Runtime>::Delegate {
			delegator: [0xf5; 20].into(),
			operator: AccountId32::new([0xF4; 32]),
		}
		.encode();
		assert_eq!(
			sc_call_delegate,
			hex::decode("00f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4")
				.unwrap()
		);
	}
}
