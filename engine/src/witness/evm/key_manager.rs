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

use alloy_consensus::Transaction as _;
use alloy_network::TransactionResponse;
use cf_chains::evm::{
	EvmChain, EvmTransactionMetadata, SchnorrVerificationComponents, TransactionFee,
};
use sp_core::H256;

use crate::{evm::conversions::IntoSpCore, witness::evm::EvmTransactionClient};

alloy_sol_types::sol!(
	#[sol(all_derives)]
	KeyManager,
	concat!(
		env!("CF_ETH_CONTRACT_ABI_ROOT"),
		"/",
		env!("CF_ETH_CONTRACT_ABI_TAG"),
		"/IKeyManager.json"
	)
);

pub use KeyManager::{AggKeySetByGovKey, GovernanceAction, KeyManagerEvents, SignatureAccepted};

// This type is generated in the macro above.
//`Key(uint256,uint8)`
impl IShared::Key {
	/// 1 byte of pub_key_y_parity followed by 32 bytes of pub_key_x
	/// Equivalent to secp256k1::PublicKey.serialize()
	pub fn serialize(&self) -> [u8; 33] {
		let mut bytes: [u8; 33] = [0; 33];
		bytes[0] = match self.pubKeyYParity {
			0 => 2,
			_ => 3,
		};
		bytes[1..33].copy_from_slice(&self.pubKeyX.to_be_bytes::<32>());
		bytes
	}
}

use anyhow::Result;

use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_vaults::VaultKeyRotatedExternally;
use state_chain_runtime::chainflip::witnessing::pallet_hooks::{self, EvmKeyManagerEvent};

use crate::evm::event::Event;

////////////////////////////////////////////////////////
// Elections code

pub async fn handle_key_manager_events<
	T: pallet_hooks::Config<I, TargetChain: EvmChain>,
	I: 'static,
>(
	client: &impl EvmTransactionClient,
	events: Vec<Event<KeyManagerEvents>>,
	block_height: u64,
) -> Result<Vec<EvmKeyManagerEvent<T, I>>> {
	Ok(futures::future::try_join_all(events.into_iter().map(|event| {
		handle_key_manager_event::<T, I>(
			client,
			event.event_parameters,
			event.tx_hash,
			block_height,
		)
	}))
	.await?
	.into_iter()
	.flatten()
	.collect())
}

async fn handle_key_manager_event<T: pallet_hooks::Config<I, TargetChain: EvmChain>, I: 'static>(
	client: &impl EvmTransactionClient,
	event: KeyManagerEvents,
	tx_hash: H256,
	block_height: u64,
) -> Result<Option<EvmKeyManagerEvent<T, I>>> {
	Ok(Some(match event {
		KeyManagerEvents::AggKeySetByGovKey(AggKeySetByGovKey {
			newAggKey: new_agg_key, ..
		}) => EvmKeyManagerEvent::AggKeySetByGovKey(VaultKeyRotatedExternally {
			new_public_key: cf_chains::evm::AggKey::from_pubkey_compressed(new_agg_key.serialize()),
			block_number: block_height,
			tx_id: tx_hash,
		}),
		KeyManagerEvents::SignatureAccepted(SignatureAccepted { sigData: sig_data, .. }) => {
			let receipt = client.transaction_receipt(tx_hash).await?;
			let (gas_used, effective_gas_price) =
				(receipt.gas_used.into(), receipt.effective_gas_price);

			let transaction = client.get_transaction(tx_hash).await?;
			let tx_metadata = EvmTransactionMetadata {
				// A contract-creation transaction has no `to`, but anyone can submit a signed call
				// from a constructor. The signature was still consumed, so we report it, with an
				// address that won't match the expected contract, so no fee is refunded.
				contract: receipt.to.unwrap_or_default().into_sp_core(),
				// Only dynamic-fee transactions carry the EIP-1559 fee fields.
				max_fee_per_gas: transaction
					.is_dynamic_fee()
					.then(|| alloy_consensus::Transaction::max_fee_per_gas(&transaction).into()),
				max_priority_fee_per_gas: alloy_consensus::Transaction::max_priority_fee_per_gas(
					&transaction,
				)
				.map(Into::into),
				gas_limit: Some(transaction.gas_limit().into()),
			};

			EvmKeyManagerEvent::SignatureAccepted(TransactionConfirmation {
				tx_out_id: SchnorrVerificationComponents {
					s: sig_data.sig.to_be_bytes::<32>(),
					k_times_g_address: sig_data.kTimesGAddress.into(),
				},
				signer_id: receipt.from.into_sp_core(),
				tx_fee: TransactionFee { effective_gas_price, gas_used },
				tx_metadata,
				transaction_ref: TransactionResponse::tx_hash(&transaction).into_sp_core(),
			})
		},
		KeyManagerEvents::GovernanceAction(GovernanceAction { message: call_hash }) =>
			EvmKeyManagerEvent::SetWhitelistedCallHash(call_hash.0),
		_ => return Ok(None),
	}))
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloy_network::{AnyRpcTransaction, AnyTransactionReceipt};
	use alloy_primitives::U256;
	use cf_chains::evm::Address;
	use serde_json::json;
	use state_chain_runtime::{EthereumInstance, Runtime};

	struct ContractCreationClient;

	const ZERO_HASH: &str = "0x0000000000000000000000000000000000000000000000000000000000000000";
	const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

	#[async_trait::async_trait]
	impl EvmTransactionClient for ContractCreationClient {
		async fn transaction_receipt(&self, _tx_hash: H256) -> Result<AnyTransactionReceipt> {
			Ok(serde_json::from_value(json!({
				"transactionHash": ZERO_HASH,
				"transactionIndex": "0x0",
				"blockHash": ZERO_HASH,
				"blockNumber": "0x0",
				"from": ZERO_ADDRESS,
				"to": null,
				"contractAddress": ZERO_ADDRESS,
				"cumulativeGasUsed": "0x1",
				"gasUsed": "0x1",
				"effectiveGasPrice": "0x1",
				"logs": [],
				"logsBloom": format!("0x{}", "00".repeat(256)),
				"status": "0x1",
				"type": "0x2",
			}))?)
		}

		async fn get_transaction(&self, _tx_hash: H256) -> Result<AnyRpcTransaction> {
			Ok(serde_json::from_value(json!({
				"hash": ZERO_HASH,
				"nonce": "0x0",
				"blockHash": null,
				"blockNumber": null,
				"transactionIndex": null,
				"from": ZERO_ADDRESS,
				"to": null,
				"value": "0x0",
				"gas": "0x0",
				"maxFeePerGas": "0x0",
				"maxPriorityFeePerGas": "0x0",
				"input": "0x",
				"chainId": "0x1",
				"accessList": [],
				"yParity": "0x0",
				"v": "0x0",
				"r": "0x1",
				"s": "0x1",
				"type": "0x2",
			}))?)
		}
	}

	#[tokio::test]
	async fn signature_accepted_by_a_contract_creation_is_witnessed() {
		let event = handle_key_manager_event::<Runtime, EthereumInstance>(
			&ContractCreationClient,
			KeyManagerEvents::SignatureAccepted(SignatureAccepted {
				sigData: IShared::SigData {
					sig: U256::from(1),
					nonce: U256::ZERO,
					kTimesGAddress: Default::default(),
				},
				signer: Default::default(),
			}),
			H256::zero(),
			0,
		)
		.await
		.unwrap();

		assert!(matches!(
			event,
			Some(EvmKeyManagerEvent::SignatureAccepted(TransactionConfirmation {
				tx_metadata: EvmTransactionMetadata { contract, .. },
				..
			})) if contract == Address::zero()
		));
	}
}
