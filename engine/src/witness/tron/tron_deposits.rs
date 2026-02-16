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

use crate::{tron::retry_rpc::TronRetryRpcApi, witness::evm::vault::decode_cf_parameters};
use anyhow::ensure;
use cf_chains::{
	address::EncodedAddress, assets::any::Asset, eth::Address as EthAddress, CcmAdditionalData,
	CcmChannelMetadata, CcmDepositMetadata, CcmMessage, ForeignChain, ForeignChainAddress,
};
use cf_primitives::{
	GasAmount, /* AccountId, AffiliateShortId, Affiliates, Beneficiary, DcaParameters */
};
use codec::{Decode, Encode};
use ethers::types::H160;
// use pallet_cf_ingress_egress::VaultDepositWitness;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// This is the encoded data that the Tron memo/note must have.
#[derive(
	Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TypeInfo, Encode, Decode, PartialOrd, Ord,
)]
pub struct TronVaultSwapData {
	// pub input_asset: Asset, // This is interpreted from the tx
	pub output_asset: Asset,
	// pub deposit_amount: u64, // This is interpreted from the tx
	pub destination_address: EncodedAddress,
	// We follow the same approach as for EVM Vault contract events where the user
	// passes a GasAmount and CcmMessage in the data and then decoding the cd_parameters
	// will give us the ccmAdditionalData to complete the deposit_metadata.
	// pub deposit_metadata: Option<CcmDepositMetadataUnchecked<ForeignChainAddress>>,
	pub ccm_data: Option<(GasAmount, CcmMessage)>,
	// pub tx_id: H256, // This is interpreted from the tx
	// pub deposit_address: EthAddress, // This will be None for Tron vault swaps
	pub cf_parameters: Vec<u8>,
	// These will be decodec from DCA Parameters
	// pub broker_fee: Beneficiary<AccountId>,
	// pub refund_params: ChannelRefundParametersForChain<Tron>,
	// pub dca_params: Option<DcaParameters>,
	// pub boost_fee: u8,
	// pub affiliate_fees: Affiliates<AffiliateShortId>,
}

/// Query block balance information from the Tron blockchain and calculate
/// balance changes for specific deposit channels and vault address.
/// This function retrieves the balance trace for a specific block,
/// filters for successful transactions, and accumulates balance changes
/// per transaction for the provided deposit channels and vault address.
/// Returns two vectors:
/// - First: (transaction_id, evm_address, amount) for each deposit channel change
/// - Second: (transaction_id, vault_amount) for vault changes
///
/// Transactions with any negative deposit channel amounts are skipped entirely.
pub async fn ingress_amounts<TronRetryRpcClient>(
	tron_rpc: &TronRetryRpcClient,
	deposit_channels: &[H160],
	vault_address: H160,
	block_number: i64,
	block_hash: &str,
) -> Result<(Vec<(String, H160, u64)>, Vec<(String, u64)>), anyhow::Error>
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

	let mut deposit_channel_changes: Vec<(String, H160, u64)> = Vec::new();
	let mut vault_changes: Vec<(String, u64)> = Vec::new();

	// Iterate over transaction balance traces
	'transaction_loop: for tx_trace in block_balance.transaction_balance_trace {
		// Skip transactions that are not successful
		if tx_trace.status != "SUCCESS" {
			continue;
		}

		let tx_id = tx_trace.transaction_identifier.clone();
		let mut channel_balances: HashMap<H160, u64> = HashMap::new();
		let mut vault_balance: u64 = 0;

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
				*channel_balances.entry(evm_addr).or_insert(0) += operation.amount as u64;
			} else if evm_addr == vault_address {
				// If any operation for the vault has negative amount, skip this transaction.
				// Valid native Vault swaps will always transfer a positive amount to the Vault.
				if operation.amount < 0 {
					continue 'transaction_loop;
				}
				vault_balance += operation.amount as u64;
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

/// Query block balance information and fetch transaction details for vault ingresses.
/// This function calls `ingress_amounts` to get deposit channel and vault changes,
/// then fetches and validates transaction information for each vault ingress.
/// Returns:
/// - deposit_channel_changes: Vec of (transaction_id, evm_address, amount)
/// - vault_swaps: Vec of Vault swaps
pub async fn ingress_deposit_channels_and_vault_swaps<TronRetryRpcClient>(
	tron_rpc: &TronRetryRpcClient,
	deposit_channels: &[H160],
	vault_address: H160,
	block_number: i64,
	block_hash: &str,
) -> Result<(Vec<(String, H160, u64)>, Vec<String>), anyhow::Error>
where
	TronRetryRpcClient: TronRetryRpcApi + Send + Sync + Clone,
{
	// Get ingress amounts for deposit channels and vault
	let (deposit_channel_changes, vault_changes) =
		ingress_amounts(tron_rpc, deposit_channels, vault_address, block_number, block_hash)
			.await?;

	println!("vault_changes: {:?}", vault_changes);
	// Fetch transaction data for each vault ingress and extract raw_data.data
	let mut vault_swaps = Vec::new();

	// TODO: We should do the processing in parallel.
	// TODO: We need t get the ERC20 events to know vault changes for TRC-20 tokens. We
	// then need to query for the transactions the same way as for the TRX Vault swaps.
	for (tx_id, amount) in vault_changes {
		let transaction = tron_rpc.get_transaction_by_id(&tx_id).await;

		// TODO: We could have the amount in the encoded payload (memo) or not. If so, we
		// would then need to check it against the amount from the balance trace or the
		// TRC20 event to make sure it matches. For now it seems unnecessary to ask for
		// duplicated data so we just rely on events (ERC20) or balance trace (TRX).
		if transaction.tx_id == tx_id {
			if let Some(raw_data) = transaction.raw_data.data {
				println!("Raw data for transaction {:?}", raw_data);
				if let Ok(bytes) = hex::decode(&raw_data) {
					println!("bytes for transaction {:?}", bytes);
					if let Ok(utf8_string) = String::from_utf8(bytes.clone()) {
						println!("UTF-8 string: {}", utf8_string);
					}
					if let Ok(details) = TronVaultSwapData::decode(&mut &bytes[..]) {
						println!("Decoded TronVaultSwapData: {:?}", details);
						// Decode cf_parameters and build deposit_metadata based on whether CCM data
						// is present
						let (vault_swap_params, deposit_metadata) =
							if let Some((gas_budget, message)) = details.ccm_data.as_ref() {
								let (vault_swap_params, ccm_additional_data) =
									match decode_cf_parameters::<EthAddress, CcmAdditionalData>(
										&details.cf_parameters,
										block_number as u64,
									) {
										Ok(result) => result,
										Err(e) => {
											println!("Failed to decode cf_parameters with CCM for tx {}: {:?}", tx_id, e);
											continue; // Skip this transaction and process the next one
										},
									};
								println!("Successfully decoded cf_parameters with CCM:");
								println!("  ccm_additional_data: {:?}", ccm_additional_data);

								let deposit_metadata =
									Some(CcmDepositMetadata::<ForeignChainAddress, _> {
										source_chain: ForeignChain::Tron,
										source_address: Default::default(), /* No source address
										                                     * for Tron Vault
										                                     * Swaps */
										channel_metadata: CcmChannelMetadata {
											message: message.clone(),
											gas_budget: *gas_budget,
											ccm_additional_data,
										},
									});
								(vault_swap_params, deposit_metadata)
							} else {
								let (vault_swap_params, ()) =
									match decode_cf_parameters::<EthAddress, ()>(
										&details.cf_parameters,
										block_number as u64,
									) {
										Ok(result) => result,
										Err(e) => {
											println!("Failed to decode cf_parameters without CCM for tx {}: {:?}", tx_id, e);
											continue; // Skip this transaction and process the next one
										},
									};
								println!("Successfully decoded cf_parameters without CCM");
								(vault_swap_params, None)
							};

						println!("  broker_fee: {:?}", vault_swap_params.broker_fee);
						println!("  affiliate_fees: {:?}", vault_swap_params.affiliate_fees);
						println!("  refund_params: {:?}", vault_swap_params.refund_params);
						println!("  dca_params: {:?}", vault_swap_params.dca_params);
						println!("  boost_fee: {:?}", vault_swap_params.boost_fee);
						println!("  deposit_metadata: {:?}", deposit_metadata);
						println!("  amount: {:?}", amount);

						// TODO To push the whole Vault swap and/or vote on that.
						// (VaultDepositWitness type)
						vault_swaps.push(tx_id.clone());

						// vault_swaps.push(crate::witness::evm::vault::vault_deposit_witness!(
						// 	Asset::Trx, // Use Trx or USDT
						// 	amount,
						// 	details.output_asset,
						// 	details.destination_address,
						// 	deposit_metadata,
						// 	tx_id,
						// 	vault_swap_params
						// ));
					}
				}
			}
		}
	}

	Ok((deposit_channel_changes, vault_swaps))
}

#[cfg(test)]
mod tests {
	use crate::{
		tron::{
			retry_rpc::{TronEndpoints, TronRetryRpcClient},
			rpc_client_api::TronAddress,
		},
		witness::tron::tron_deposits::{ingress_amounts, ingress_deposit_channels_and_vault_swaps},
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

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_ingress_deposit_channels_and_vault_swaps() {
		task_scope::task_scope(|scope| {
			async {
				let retry_client = TronRetryRpcClient::new(
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
					3448148188,
					1,
				)
				.await
				.unwrap();

				// Test block - update these values
				let block_num = 64843264;
				let block_hash = "0000000003dd6e006934d46981dab0f3cf1863b6d7b0a50577e198e06bb8560b";

				// No deposit channels
				let deposit_channels = vec![];

				// Test vault address - update this value
				let vault_address = TronAddress(
					hex::decode("41c34856cadd5524892907d8a34126053447740375")
						.unwrap()
						.try_into()
						.unwrap(),
				)
				.to_evm_address()
				.unwrap();

				let (deposit_channel_changes, vault_swaps) =
					ingress_deposit_channels_and_vault_swaps(
						&retry_client,
						&deposit_channels,
						vault_address,
						block_num,
						block_hash,
					)
					.await
					.unwrap();

				println!("Deposit channel changes: {:?}", deposit_channel_changes);
				println!("Vault swaps: {:?}", vault_swaps);

				// Update assertions based on expected results
				assert_eq!(deposit_channel_changes.len(), 0);

				// This cointains a TRX Vault Swap with valid data
				// https://nile.tronscan.org/#/transaction/b8042280e6a813d65ad01a0555e1e9a9497bf69d012b58cdc5d925c21df35972
				let (deposit_channel_changes, vault_swaps) =
					ingress_deposit_channels_and_vault_swaps(
						&retry_client,
						&deposit_channels,
						vault_address,
						64845362,
						"0000000003dd7632dd9fcdfcbe8008f7e534191ff5d1ceedb05ac5affdf76b32",
					)
					.await
					.unwrap();

				println!("Deposit channel changes: {:?}", deposit_channel_changes);
				println!("Vault swaps: {:?}", vault_swaps);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[test]
	fn test_encode_tron_vault_swap_data() {
		use crate::witness::tron::tron_deposits::TronVaultSwapData;
		use cf_chains::{address::EncodedAddress, assets::any::Asset};
		use codec::Encode;

		// Create a sample TronVaultSwapData without CCM
		let vault_swap_data = TronVaultSwapData {
			output_asset: Asset::Eth,
			destination_address: EncodedAddress::Eth([0x12; 20]),
			ccm_data: None,
			// Valid Cf_parameters
			cf_parameters: hex::decode("0164000000f627b6285759e4fa9ca1214c31f6748afaad766c6ccf732256d0ecbe06e43a03000000000000000000000000000000000000000000016e00000070d0cd75a367987344a3896a18e1510e5429ca5e88357b6c2a2e306b3877380d000000").unwrap(),
		};

		let encoded = vault_swap_data.encode();
		let hex_encoded = hex::encode(&encoded);

		println!("Encoded TronVaultSwapData (no CCM):");
		println!("  Hex: {}", hex_encoded);
		println!("  Bytes: {:?}", encoded);
		println!("  Length: {} bytes", encoded.len());

		// TODO We need to reencode cf_parameters for that.

		// Create a sample TronVaultSwapData with CCM
		// let vault_swap_data_with_ccm = TronVaultSwapData {
		// 	output_asset: Asset::Usdc,
		// 	destination_address: EncodedAddress::Eth([0xab; 20]),
		// 	ccm_data: Some((1000000u128, vec![0x48, 0x65, 0x6c, 0x6c, 0x6f].try_into().unwrap())),
		// 	cf_parameters: vec![0x04, 0x05, 0x06, 0x07],
		// };

		// let encoded_with_ccm = vault_swap_data_with_ccm.encode();
		// let hex_encoded_with_ccm = hex::encode(&encoded_with_ccm);

		// println!("\nEncoded TronVaultSwapData (with CCM):");
		// println!("  Hex: {}", hex_encoded_with_ccm);
		// println!("  Bytes: {:?}", encoded_with_ccm);
		// println!("  Length: {} bytes", encoded_with_ccm.len());

		// // Verify round-trip decoding
		// let decoded: TronVaultSwapData =
		// 	codec::Decode::decode(&mut &encoded[..]).expect("Should decode successfully");
		// assert_eq!(decoded, vault_swap_data);

		// let decoded_with_ccm: TronVaultSwapData =
		// 	codec::Decode::decode(&mut &encoded_with_ccm[..])
		// 		.expect("Should decode successfully");
		// assert_eq!(decoded_with_ccm, vault_swap_data_with_ccm);

		// println!("\nRound-trip encoding/decoding successful!");
	}
}
