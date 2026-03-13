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

use crate::{
	tron::cached_rpc::TronRetryRpcApiWithResult,
	witness::{
		eth_elections::EvmSingleBlockQuery,
		evm::{erc20_deposits::Erc20Events::TransferFilter, EvmBlockQuery},
		tron::TronDepositChannelWitnessingConfig,
	},
};

use anyhow::{ensure, Result};
use cf_chains::{evm::DepositDetails, Chain, DepositChannel, Tron};
use ethers::types::{H160, H256};
use std::collections::{HashMap, HashSet};

/// Query block balance information from the Tron blockchain and calculate
/// TRX balance changes for specific deposit channels.
/// Transactions with any negative deposit channel amounts are skipped entirely.
pub async fn trx_ingress_amounts<Client>(
	client: &Client,
	trx_deposit_channels: HashSet<H160>,
	block_number: i64,
	block_hash: &str,
) -> Result<Vec<(H160, u64, H256)>, anyhow::Error>
where
	Client: TronRetryRpcApiWithResult + Send + Sync + Clone,
{
	if trx_deposit_channels.is_empty() {
		return Ok(Vec::new());
	}

	let block_balance = client.get_block_balances(block_number, block_hash).await?;

	ensure!(
		block_balance.block_identifier.hash == block_hash,
		"Block hash mismatch: expected {}, got {}",
		block_hash,
		block_balance.block_identifier.hash
	);
	if let Some(number) = block_balance.block_identifier.number {
		ensure!(
			number == block_number,
			"Block number mismatch: expected {}, got {}",
			block_number,
			number
		);
	}

	let mut deposit_channel_changes: Vec<(H160, u64, H256)> = Vec::new();

	// Iterate over transaction balance traces
	'transaction_loop: for tx_trace in block_balance.transaction_balance_trace {
		// Skip transactions that are not successful, even if they should be succesfull as the
		// balance changed
		if tx_trace.status != "SUCCESS" {
			tracing::warn!(
				"Skipping Tron transaction with non-success status: tx_id={}, status={}",
				tx_trace.transaction_identifier,
				tx_trace.status
			);
			continue;
		}

		let tx_id = tx_trace.transaction_identifier;
		let mut channel_balances: HashMap<H160, u64> = HashMap::new();

		// Iterate over operations in this transaction and accumulate amounts to deposit channels.
		// A transaction might deposit to multiple deposit channels. We accumulate multiple deposits
		// to the same channel in a single item. We skip any transactions that subtract any
		// amount from the deposit channels, as that would indicate a fetch transaction.
		// It's technically  possible that an allBatch transaction transfers to a deposit channel
		// but in reality that never happens. It just seems safer to just skip all allBatch txs.
		for operation in tx_trace.operation {
			let evm_addr = operation.address.to_evm_address().map_err(|e| {
				anyhow::anyhow!("Failed to convert Tron address to EVM address: {}", e)
			})?;

			if trx_deposit_channels.contains(&evm_addr) {
				// If any operation for a deposit channel has negative amount, skip this
				// transaction. That means it's a fetch (allBatch) transaction.
				if operation.amount < 0 {
					continue 'transaction_loop;
				}
				*channel_balances.entry(evm_addr).or_insert(0) += operation.amount as u64;
			}
		}

		// Add the deposit channel changes to the result vector
		for (channel_address, amount) in channel_balances {
			deposit_channel_changes.push((channel_address, amount, tx_id));
		}
	}

	Ok(deposit_channel_changes)
}

pub async fn witness_deposit_channels<Client: TronRetryRpcApiWithResult + Send + Sync + Clone>(
	client: &Client,
	config: &TronDepositChannelWitnessingConfig,
	query: &EvmSingleBlockQuery,
	deposit_addresses: Vec<DepositChannel<Tron>>,
) -> Result<Vec<pallet_cf_ingress_egress::DepositWitness<Tron>>> {
	use itertools::Itertools;
	use pallet_cf_ingress_egress::DepositWitness;

	let (trx_deposit_channels, erc20_deposit_channels): (Vec<_>, HashMap<_, Vec<_>>) =
		deposit_addresses.into_iter().fold(
			(Vec::new(), HashMap::new()),
			|(mut trx, mut erc20), deposit_channel| {
				let asset = deposit_channel.asset;
				let address = deposit_channel.address;
				if asset == Tron::GAS_ASSET {
					trx.push((address, deposit_channel.state));
				} else {
					erc20.entry(asset).or_insert_with(Vec::new).push(address);
				}
				(trx, erc20)
			},
		);
	let trx_deposit_addresses: HashSet<H160> =
		trx_deposit_channels.iter().map(|(address, _state)| *address).collect();

	let block_number_u64 = query.get_lowest_block_height_of_query();
	let block_number = i64::try_from(block_number_u64).map_err(|_| {
		anyhow::anyhow!("Block number conversion to i64 failed: value too large or negative")
	})?;
	let block_hash = format!("{:064x}", query.block_hash);

	let deposit_channel_changes =
		trx_ingress_amounts(client, trx_deposit_addresses, block_number, &block_hash).await?;

	if deposit_channel_changes.len() > 0 {
		println!("DALEDALE Tron deposit channel changes: {:?}", deposit_channel_changes);
	}

	// --- ERC20 deposit channel witnessing ---
	let mut erc20_ingresses: Vec<DepositWitness<Tron>> = Vec::new();

	// Handle each asset type separately with its specific event type
	for (asset, deposit_channels) in erc20_deposit_channels {
		let event_source = config.supported_assets.get(&asset).ok_or_else(|| {
			anyhow::anyhow!("Tried to get erc20 events for unsupported asset: {asset:?}")
		})?;

		let logs = client.get_logs(query.block_hash, event_source.contract_address).await?;
		let events: Vec<_> = logs
			.into_iter()
			.filter_map(|log| event_source.event_type.parse_log(log).ok())
			.collect();

		let asset_ingresses = events
			.into_iter()
			.filter_map(|event| match event.event_parameters {
				TransferFilter { to, value, from: _ } if deposit_channels.contains(&to) =>
					Some(DepositWitness {
						deposit_address: to,
						amount: value.try_into().expect(
							"Any ERC20 tokens we support should have amounts that fit into a u128",
						),
						asset,
						deposit_details: DepositDetails { tx_hashes: Some(vec![event.tx_hash]) },
					}),
				_ => None,
			})
			.collect::<Vec<_>>();

		erc20_ingresses.extend(asset_ingresses);
	}

	if erc20_ingresses.len() > 0 {
		println!("DALEDALE Tron ERC20 ingresses: {:?}", erc20_ingresses);
	}

	Ok(deposit_channel_changes
		.into_iter()
		.map(|(channel_address, amount, tx_id)| DepositWitness {
			deposit_address: channel_address,
			asset: Tron::GAS_ASSET,
			amount: amount.into(),
			deposit_details: DepositDetails { tx_hashes: Some(vec![tx_id]) },
		})
		.chain(erc20_ingresses)
		.sorted_by_key(|deposit_witness| deposit_witness.deposit_address)
		.collect())
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::TronEndpoints,
		tron::{retry_rpc::TronRetryRpcClient, rpc_client_api::TronAddress},
		witness::tron::tron_deposits::trx_ingress_amounts,
	};
	use cf_utilities::{redact_endpoint_secret::SecretUrl, task_scope};
	use ethers::types::H160;
	use futures_util::FutureExt;
	use std::collections::HashSet;

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_ingress_deposit_channels_empty() {
		task_scope::task_scope(|scope| {
			async {
				let retry_client = TronRetryRpcClient::<crate::tron::rpc::TronRpcClient>::new(
					scope,
					crate::settings::NodeContainer {
						primary: TronEndpoints {
							http_endpoint: SecretUrl::from(
								"https://nile.trongrid.io/wallet".to_string(),
							),
							json_rpc_endpoint: SecretUrl::from(
								"https://nile.trongrid.io/jsonrpc".to_string(),
							),
						},
						backup: None,
					},
					ethers::types::U256::from(3448148188u64), // Nile testnet chain ID (0xcd8690dc)
					"tron_rpc",
					"Tron",
				)
				.await
				.unwrap();

				// Test block - update these values
				let block_num = 64843264;
				let block_hash = "0000000003dd6e006934d46981dab0f3cf1863b6d7b0a50577e198e06bb8560b";

				// Example address that is unused
				let deposit_channels_tron = [TronAddress(
					hex::decode("41a7bd91a81449253dd0ee8c51c04e0578be6c4a90")
						.unwrap()
						.try_into()
						.unwrap(),
				)];
				let deposit_channels: HashSet<_> = deposit_channels_tron
					.iter()
					.map(|addr| addr.to_evm_address().unwrap())
					.collect();

				let deposit_channel_changes =
					trx_ingress_amounts(&retry_client, deposit_channels, block_num, block_hash)
						.await
						.unwrap();

				println!("Deposit channel changes: {:?}", deposit_channel_changes);

				assert_eq!(deposit_channel_changes.len(), 0);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_get_tron_trx_ingress_amounts() {
		task_scope::task_scope(|scope| {
			async {
				let retry_client = TronRetryRpcClient::<crate::tron::rpc::TronRpcClient>::new(
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
					ethers::types::U256::from(728126428u64), // Mainnet chain ID (0x2b6653dc)
					"tron_rpc",
					"Tron",
				)
				.await
				.unwrap();

				// Test block from mainnet
				let block_num = 80079354i64;
				let block_hash = "0000000004c5e9fa0b5bff64330976a20f1e5007f66f3f0524168a782d998945";

				// Example deposit channels - Tron addresses (21 bytes, with 0x41 prefix)
				let deposit_channels_tron = [
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
				let deposit_channels: HashSet<_> = deposit_channels_tron
					.iter()
					.map(|addr| addr.to_evm_address().unwrap())
					.collect();

				let deposit_channel_changes = trx_ingress_amounts(
					&retry_client,
					deposit_channels.clone(),
					block_num,
					block_hash,
				)
				.await
				.unwrap();

				assert_eq!(
					deposit_channel_changes,
					vec![(
						H160::from_slice(
							&hex::decode("595aeac7a37b75c0abe0561e1390c748b5dc4ca2").unwrap()
						),
						3,
						"faaaba965bce89c1cb28cada1615d75d2e3c3a05970e8a3bbc296a1239d411e2"
							.parse()
							.unwrap()
					)]
				);

				// Test with vault address that has negative change (should be skipped)
				let deposit_channel_changes = trx_ingress_amounts(
					&retry_client,
					deposit_channels.clone(),
					block_num,
					block_hash,
				)
				.await
				.unwrap();

				assert_eq!(
					deposit_channel_changes,
					vec![(
						H160::from_slice(
							&hex::decode("595aeac7a37b75c0abe0561e1390c748b5dc4ca2").unwrap()
						),
						3,
						"faaaba965bce89c1cb28cada1615d75d2e3c3a05970e8a3bbc296a1239d411e2"
							.parse()
							.unwrap()
					)]
				);
				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
