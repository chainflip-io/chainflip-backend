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
use cf_primitives::AccountId;
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
use codec::{Decode, Encode, MaxEncodedLen};
use futures_core::Future;
use scale_info::TypeInfo;

abigen!(ScUtils, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IScUtils.json");

use anyhow::Result;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub enum ScCall {
	// TODO: What type should the `sc_account` be?
	DelegateTo { sc_account: AccountId },
}

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

			let _process_call = process_call.clone();
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
					let _call: state_chain_runtime::RuntimeCall = match event.event_parameters {
						ScUtilsEvents::DepositToScGatewayAndScCallFilter(DepositToScGatewayAndScCallFilter {
							sender, // eth_address to attribute the FLIP to
                            signer, // eth_address that signed the call. Not to be used for now
                            amount, // amount of FLIP deposited
                            sc_call
						}) => {
                            match ScCall::decode(&mut &sc_call[..]) {
                                Ok(ScCall::DelegateTo { sc_account }) => {
									println!("Successfully Decoded ScCall! {:?}, amount: {amount}, sender: {sender}, signer: {signer}, epoch index {:?}", sc_account, epoch.index);
                                    trace!("Successfully decoded ScCall::DelegateTo with sc_account: {sc_account}");
                                },
                                Err(_e) => {
									println!("Failed to decode ScCall");
                                }
                            }
                            continue
                        },
						_ => {
							trace!("Ignoring unused event: {event}");
							continue
						},
					};
                    // TODO: To add once we have something to call
					// process_call(call, epoch.index).await;
				}

				Result::Ok(header.data)
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_sc_call_encode() {
		let sc_call_delegate =
			ScCall::DelegateTo { sc_account: AccountId::new([0xF4; 32]) }.encode();
		assert_eq!(
			sc_call_delegate,
			hex::decode("00f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4")
				.unwrap()
		);
	}
}
