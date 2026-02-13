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

use crate::tron::retry_rpc::TronRetryRpcApi;
use anyhow::ensure;
use ethers::types::H160;
use std::collections::HashMap;

/// Query block balance information from the Tron blockchain and calculate
/// balance changes for specific deposit channels and vault address.
/// This function retrieves the balance trace for a specific block,
/// filters for successful transactions, and accumulates balance changes
/// per transaction for the provided deposit channels and vault address.
/// Returns two vectors:
/// - First: (transaction_id, evm_address, amount) for each deposit channel change
/// - Second: (transaction_id, vault_amount) for vault changes
/// Transactions with any negative deposit channel amounts are skipped entirely.
pub async fn ingress_amounts<TronRetryRpcClient>(
	tron_rpc: &TronRetryRpcClient,
	deposit_channels: &[H160],
	vault_address: H160,
	block_number: i64,
	block_hash: &str,
) -> Result<(Vec<(String, H160, i64)>, Vec<(String, i64)>), anyhow::Error>
where
	TronRetryRpcClient: TronRetryRpcApi + Send + Sync + Clone,
{
	let block_balance = tron_rpc.get_block_balances(block_number, block_hash).await;

	// Check that block identifier matches
	ensure!(
		block_balance.block_identifier.hash == block_hash,
		"Block hash mismatch: expected {}, got {}",
		block_hash,
		block_balance.block_identifier.hash
	);
	ensure!(
		block_balance.block_identifier.number == block_number,
		"Block number mismatch: expected {}, got {}",
		block_number,
		block_balance.block_identifier.number
	);

	let mut deposit_channel_changes: Vec<(String, H160, i64)> = Vec::new();
	let mut vault_changes: Vec<(String, i64)> = Vec::new();

	// Iterate over transaction balance traces
	'transaction_loop: for tx_trace in block_balance.transaction_balance_trace {
		// Skip transactions that are not successful
		if tx_trace.status != "SUCCESS" {
			continue;
		}

		let tx_id = tx_trace.transaction_identifier.clone();
		let mut channel_balances: HashMap<H160, i64> = HashMap::new();
		let mut vault_balance: i64 = 0;

		// Iterate over operations in this transaction and accumulate amounts to deposit channel and
		// Vault. A transaction might deposit to multiple deposit channels and even start a Vault
		// swap. We accumulate multiple deposits to the same channel in a single item. Same for
		// Vault swaps - we've never really seen a single transaction initiating multiple Vault
		// swaps.
		// We skip fetch and transfer (allBatch) transactions. It could technically be possible
		// that an allBatch transaction transfers to a deposit channel but in reality that
		// never happens. It just seems safer to just skip all our allBatch transactions.
		for operation in tx_trace.operation {
			// Convert TronAddress to EVM address - addresses being a valid length (TronAddress)
			// is already validated by RPC layer
			let evm_addr = operation
				.address
				.to_evm_address()
				.expect("Address should have valid 0x41 prefix");

			if deposit_channels.contains(&evm_addr) {
				// If any operation for a deposit channel has negative amount, skip this
				// transaction. That means it's a fetch (allBatch) transaction.
				if operation.amount < 0 {
					continue 'transaction_loop;
				}
				*channel_balances.entry(evm_addr).or_insert(0) += operation.amount;
			} else if evm_addr == vault_address {
				// If any operation for the vault has negative amount, skip this transaction.
				// Valid native Vault swaps will always transfer a positive amount to the Vault.
				if operation.amount < 0 {
					continue 'transaction_loop;
				}
				vault_balance += operation.amount;
			}
		}

		// Add the deposit channel changes to the result vector
		for (channel, amount) in channel_balances {
			deposit_channel_changes.push((tx_id.clone(), channel, amount));
		}

		// Add the vault change to the result vector (even if 0)
		if vault_balance != 0 {
			vault_changes.push((tx_id, vault_balance));
		}
	}

	Ok((deposit_channel_changes, vault_changes))
}

#[cfg(test)]
mod tests {
	use crate::{
		tron::{
			retry_rpc::{TronEndpoints, TronRetryRpcClient},
			rpc_client_api::TronAddress,
		},
		witness::tron::tron_deposits::ingress_amounts,
	};
	use cf_utilities::{redact_endpoint_secret::SecretUrl, task_scope};
	use ethers::types::H160;
	use futures_util::FutureExt;

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_get_tron_ingress_amounts() {
		task_scope::task_scope(|scope| {
			async {
				let retry_client = TronRetryRpcClient::new(
					scope,
					crate::settings::NodeContainer {
						primary: TronEndpoints {
							http_endpoint: SecretUrl::from(
								"https://docs-demo.tron-mainnet.quiknode.pro/wallet".to_string(),
							),
							json_rpc_endpoint: SecretUrl::from(
								"https://docs-demo.tron-mainnet.quiknode.pro/jsonrpc".to_string(),
							),
						},
						backup: None,
					},
					728126428, // Mainnet chain ID (0x2b6653dc)
					1,         // witness_period
				)
				.await
				.unwrap();

				// Test block from mainnet
				let block_num = 80079354i64;
				let block_hash = "0000000004c5e9fa0b5bff64330976a20f1e5007f66f3f0524168a782d998945";

				// Example deposit channels - Tron addresses (21 bytes, with 0x41 prefix)
				let deposit_channels_tron = vec![
					TronAddress(
						hex::decode("41b7bd91a81449253dd0ee8c51c04e0578be6c4a91")
							.unwrap()
							.try_into()
							.unwrap(),
					),
					TronAddress(
						hex::decode("41ac0d9820078d714da8fc6e6d9c214329f7c9daeb")
							.unwrap()
							.try_into()
							.unwrap(),
					),
					TronAddress(
						hex::decode("41004a9fd60192d8b1776cb872c09603781633431b")
							.unwrap()
							.try_into()
							.unwrap(),
					),
					TronAddress(
						hex::decode("41595aeac7a37b75c0abe0561e1390c748b5dc4ca2")
							.unwrap()
							.try_into()
							.unwrap(),
					),
				];
				let deposit_channels: Vec<_> = deposit_channels_tron
					.iter()
					.map(|addr| addr.to_evm_address().unwrap())
					.collect();

				// Test with vault address that has positive change
				let vault_address = TronAddress(
					hex::decode("4199b3b56213cd4d852cd85bf0049d2abaed17682d")
						.unwrap()
						.try_into()
						.unwrap(),
				)
				.to_evm_address()
				.unwrap();

				let (deposit_channel_changes, vault_changes) = ingress_amounts(
					&retry_client,
					&deposit_channels,
					vault_address,
					block_num,
					block_hash,
				)
				.await
				.unwrap();

				assert_eq!(
					deposit_channel_changes,
					vec![(
						"faaaba965bce89c1cb28cada1615d75d2e3c3a05970e8a3bbc296a1239d411e2"
							.to_string(),
						H160::from_slice(
							&hex::decode("595aeac7a37b75c0abe0561e1390c748b5dc4ca2").unwrap()
						),
						3
					)]
				);
				assert_eq!(
					vault_changes,
					vec![(
						"011fc77de4dd7777d1ddaa5d5411b28c250000631f8aeda0c5808d0d5134e4ca"
							.to_string(),
						2
					)]
				);

				// Test with vault address that has negative change (should be skipped)
				let vault_address = TronAddress(
					hex::decode("4104c5b113f9b4d5c836b03adcaec583be67876076")
						.unwrap()
						.try_into()
						.unwrap(),
				)
				.to_evm_address()
				.unwrap();

				let (deposit_channel_changes, vault_changes) = ingress_amounts(
					&retry_client,
					&deposit_channels,
					vault_address,
					block_num,
					block_hash,
				)
				.await
				.unwrap();

				assert_eq!(
					deposit_channel_changes,
					vec![(
						"faaaba965bce89c1cb28cada1615d75d2e3c3a05970e8a3bbc296a1239d411e2"
							.to_string(),
						H160::from_slice(
							&hex::decode("595aeac7a37b75c0abe0561e1390c748b5dc4ca2").unwrap()
						),
						3
					)]
				);
				assert_eq!(vault_changes, vec![]);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
