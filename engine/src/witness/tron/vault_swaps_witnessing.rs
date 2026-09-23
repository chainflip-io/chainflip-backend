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
	tron::{
		cached_rpc::TronRetryRpcApiWithResult,
		rpc_client_api::{TransactionResultStatus, TronAddress},
	},
	witness::{
		eth_elections::EvmSingleBlockQuery,
		evm::{
			erc20_deposits::Erc20Events::TransferFilter,
			vault::{decode_cf_parameters, vault_deposit_witness},
			EvmBlockQuery,
		},
		tron::{tron_deposits::trx_ingress_transactions, VaultDepositWitnessingConfig},
	},
};
use cf_chains::{
	address::EncodedAddress,
	assets::{any::Asset, tron::Asset as TronAsset},
	evm::{Address as EvmAddress, DepositDetails},
	CcmAdditionalData, CcmChannelMetadata, CcmDepositMetadata, CcmMessage, ForeignChain,
	ForeignChainAddress,
};
use cf_primitives::AssetAmount;
use codec::{Decode, Encode};
use ethers::types::{H256, U256};
use futures::future;
use itertools::Itertools;
use pallet_cf_ingress_egress::VaultDepositWitness;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use state_chain_runtime::{Runtime, TronInstance};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Selector of `transfer(address,uint256)`.
const TRC20_TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

#[derive(Debug, Error)]
pub enum TronFetchAndDecodeError {
	#[error("Failed to get transaction {tx_id:#x}: {source:#}")]
	GetTransaction { tx_id: H256, source: anyhow::Error },
	#[error("Transaction {tx_id:#x} has non-success status: {status:?}")]
	NonSuccessStatus { tx_id: H256, status: TransactionResultStatus },
	#[error("Transaction ID mismatch: expected {expected:?}, got {actual:?}")]
	TxIdMismatch { expected: H256, actual: H256 },
	#[error("Transaction {tx_id:#x} is not a direct transfer to the vault: {reason}")]
	NotDirectTransfer { tx_id: H256, reason: &'static str },
	#[error("Failed to decode vault swap data for tx {tx_id:#x}: {reason}")]
	DecodeVaultSwapData { tx_id: H256, reason: &'static str },
	#[error("Failed to decode cf_parameters for tx {tx_id:#x}: {reason}")]
	CfParametersDecode { tx_id: H256, reason: String },
}

// This is the encoded data that the Tron memo/note must have.
// We follow the same aproach as for EVM Vault contracts for consistency where the user
// passes a GasAmount and CcmMessage in the data and then decoding the cf_parameters
// will give us the ccmAdditionalData to complete the deposit_metadata.
#[derive(
	Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TypeInfo, Encode, Decode, PartialOrd, Ord,
)]
pub struct TronVaultSwapData {
	pub output_asset: Asset,
	pub destination_address: EncodedAddress,
	pub ccm_data: Option<(AssetAmount, CcmMessage)>,
	pub cf_parameters: Vec<u8>,
}

/// The memo of a Tron transaction is signed by whoever broadcasts it, not by whoever moved the
/// funds. Anyone can therefore attach a memo to a re-broadcast of our own signed Vault calls (for
/// example a deposit-channel fetch, which moves funds into the vault) and forge a vault swap.
///
/// We only accept a vault swap if the funds were moved by the transaction owner itself, i.e. the
/// transaction is a plain TRX `TransferContract` to the vault, or a `transfer(vault, amount)` call
/// on the token contract, and the transferred amount matches the witnessed ingress amount.
fn ensure_direct_transfer_to_vault(
	contracts: &[serde_json::Value],
	asset: TronAsset,
	amount: u64,
	vault_address: EvmAddress,
	token_contracts: &HashMap<TronAsset, EvmAddress>,
) -> Result<(), &'static str> {
	let [contract] = contracts else {
		return Err("expected exactly one contract");
	};
	let value = &contract["parameter"]["value"];
	let address = |field: &str| {
		serde_json::from_value::<TronAddress>(value[field].clone())
			.map(TronAddress::to_evm_address)
			.map_err(|_| "invalid address")
	};
	match (asset, contract["type"].as_str()) {
		(TronAsset::Trx, Some("TransferContract")) => {
			if address("to_address")? != vault_address {
				return Err("TRX transfer is not to the vault");
			}
			if value["amount"].as_u64() != Some(amount) {
				return Err("TRX amount mismatch");
			}
		},
		(token, Some("TriggerSmartContract")) => {
			let token_contract = token_contracts.get(&token).ok_or("unsupported token")?;
			if address("contract_address")? != *token_contract {
				return Err("call target is not the token contract");
			}
			// `transfer(address to, uint256 amount)` call data: a 4-byte selector followed by two
			// 32-byte ABI words.
			let data = hex::decode(value["data"].as_str().ok_or("missing call data")?)
				.map_err(|_| "invalid call data")?;
			if data.len() != 4 + 32 + 32 {
				return Err("call is not transfer(address,uint256)");
			}
			let (selector, to_word, amount_word) = (&data[..4], &data[4..36], &data[36..]);
			if selector != TRC20_TRANSFER_SELECTOR {
				return Err("call is not transfer(address,uint256)");
			}
			// An address word is the 20-byte address left-padded to 32 bytes. The TVM reads only
			// the low 20 bytes, so the padding is either all zeros or, in Tron's own encoding,
			// 11 zero bytes followed by the 0x41 address prefix. Both forms occur in the wild.
			let (padding, to) = to_word.split_at(12);
			if padding[..11] != [0u8; 11] || !matches!(padding[11], 0x00 | 0x41) {
				return Err("malformed address word");
			}
			if EvmAddress::from_slice(to) != vault_address {
				return Err("token transfer is not to the vault");
			}
			if U256::from_big_endian(amount_word) != U256::from(amount) {
				return Err("token amount mismatch");
			}
		},
		_ => return Err("unexpected contract type"),
	}
	Ok(())
}

pub async fn fetch_and_decode_transactions<Client>(
	client: &Client,
	vault_ingress_transactions: Vec<(TronAsset, u64, H256)>,
	block_number: u64,
	vault_address: EvmAddress,
	token_contracts: &HashMap<TronAsset, EvmAddress>,
) -> Vec<Result<VaultDepositWitness<Runtime, TronInstance>, TronFetchAndDecodeError>>
where
	Client: TronRetryRpcApiWithResult + Send + Sync + Clone,
{
	if vault_ingress_transactions.is_empty() {
		return Vec::new();
	}

	// Fetch all transactions in parallel for efficiency
	let get_tx_futures = vault_ingress_transactions
		.iter()
		.map(|(_, _, tx_id)| async move { client.get_transaction_by_id(*tx_id).await });
	let transactions_info_result = future::join_all(get_tx_futures).await;

	vault_ingress_transactions
		.into_iter()
		.zip(transactions_info_result)
		.filter_map(|((asset, amount, tx_id), transaction_info_result)| {
			let transaction = match transaction_info_result {
				Ok(tx) => tx,
				Err(e) =>
					return Some(Err(TronFetchAndDecodeError::GetTransaction { tx_id, source: e })),
			};

			// The transaction should not have reverted, as otherwise the value would not
			// have changed but check anyway.
			let status = transaction.status();
			if status != TransactionResultStatus::Success {
				return Some(Err(TronFetchAndDecodeError::NonSuccessStatus { tx_id, status }));
			}

			if transaction.tx_id != tx_id {
				return Some(Err(TronFetchAndDecodeError::TxIdMismatch {
					expected: tx_id,
					actual: transaction.tx_id,
				}));
			}

			// Missing memo means this transaction isn't a vault swap
			let memo = transaction.raw_data.data.as_ref()?;

			if let Err(reason) = ensure_direct_transfer_to_vault(
				&transaction.raw_data.contract,
				asset,
				amount,
				vault_address,
				token_contracts,
			) {
				return Some(Err(TronFetchAndDecodeError::NotDirectTransfer { tx_id, reason }));
			}
			let details =
				match hex::decode(memo).map_err(|_| "hex decode failed").and_then(|bytes| {
					TronVaultSwapData::decode(&mut &bytes[..]).map_err(|_| "SCALE decode failed")
				}) {
					Ok(details) => details,
					Err(reason) =>
						return Some(Err(TronFetchAndDecodeError::DecodeVaultSwapData {
							tx_id,
							reason,
						})),
				};

			// Decode cf_parameters and build deposit_metadata based on whether CCM data is present
			let (vault_swap_params, deposit_metadata) =
				if let Some((gas_budget, message)) = details.ccm_data.as_ref() {
					let (vault_swap_params, ccm_additional_data) =
						match decode_cf_parameters::<EvmAddress, CcmAdditionalData>(
							&details.cf_parameters,
							block_number,
						) {
							Ok(result) => result,
							Err(e) =>
								return Some(Err(TronFetchAndDecodeError::CfParametersDecode {
									tx_id,
									reason: format!("with CCM: {e:?}"),
								})),
						};

					let deposit_metadata = Some(CcmDepositMetadata::<ForeignChainAddress, _> {
						source_chain: ForeignChain::Tron,
						source_address: Default::default(),
						channel_metadata: CcmChannelMetadata {
							message: message.clone(),
							gas_budget: *gas_budget,
							ccm_additional_data,
						},
					});
					(vault_swap_params, deposit_metadata)
				} else {
					let (vault_swap_params, ()) = match decode_cf_parameters::<EvmAddress, ()>(
						&details.cf_parameters,
						block_number,
					) {
						Ok(result) => result,
						Err(e) =>
							return Some(Err(TronFetchAndDecodeError::CfParametersDecode {
								tx_id,
								reason: format!("without CCM: {e:?}"),
							})),
					};
					(vault_swap_params, None)
				};

			Some(Ok(vault_deposit_witness!(
				asset,
				amount.into(),
				details.output_asset,
				details.destination_address,
				deposit_metadata,
				tx_id,
				vault_swap_params
			)))
		})
		.collect()
}
pub async fn witness_vault_swaps<Client: TronRetryRpcApiWithResult + Send + Sync + Clone>(
	client: &Client,
	config: &VaultDepositWitnessingConfig,
	query: &EvmSingleBlockQuery,
) -> Result<Vec<VaultDepositWitness<Runtime, TronInstance>>, anyhow::Error> {
	let block_number = query.get_lowest_block_height_of_query();
	let vault_address = config.vault;

	let mut vault_ingress_transactions: Vec<(TronAsset, u64, H256)> = trx_ingress_transactions(
		client,
		HashSet::from([vault_address]),
		block_number,
		query.block_hash,
	)
	.await?
	.into_iter()
	.filter_map(|(addr, amount, tx_id)| {
		if addr == vault_address {
			Some((TronAsset::Trx, amount, tx_id))
		} else {
			tracing::warn!(
				"TRX ingress address mismatch: expected {:?}, got {:?}",
				vault_address,
				addr
			);
			None
		}
	})
	.collect();

	// --- ERC20 Vault swap witnessing ---

	// Iterate over all event sources in config.supported_assets
	for (asset, event_source) in &config.supported_assets {
		let logs = client.get_logs(query.block_hash, event_source.contract_address).await?;
		for event in logs.into_iter().filter_map(|log| event_source.event_type.parse_log(log).ok())
		{
			if let TransferFilter { to, value, from: _ } = event.event_parameters {
				if to == vault_address {
					vault_ingress_transactions.push((
						*asset,
						value.try_into().map_err(|_| anyhow::anyhow!("Value conversion failed"))?,
						event.tx_hash,
					));
				}
			}
		}
	}

	let token_contracts = config
		.supported_assets
		.iter()
		.map(|(asset, event_source)| (*asset, event_source.contract_address))
		.collect();

	Ok(fetch_and_decode_transactions(
		client,
		vault_ingress_transactions,
		block_number,
		vault_address,
		&token_contracts,
	)
	.await
	.into_iter()
	.filter_map(|result| match result {
		Ok(witness) => Some(witness),
		// We might submit this as a recoverable deposit in PRO-2832.
		Err(e) => {
			tracing::warn!("Skipping Tron vault swap: {e}");
			None
		},
	})
	.collect())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		settings::TronEndpoints,
		tron::{retry_rpc::TronRetryRpcClient, rpc_client_api::TronAddress},
	};
	use cf_chains::{cf_parameters::VaultSwapParameters, ChannelRefundParameters};
	use cf_utilities::{redact_endpoint_secret::SecretUrl, task_scope};
	use ethers::types::H160;
	use futures_util::FutureExt;

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_get_tron_trx_ingress_transactions() {
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
				let block_num = 80079354u64;
				let block_hash: H256 =
					"0000000004c5e9fa0b5bff64330976a20f1e5007f66f3f0524168a782d998945"
						.parse()
						.unwrap();

				// Test with vault address that has positive change
				let vault_address = TronAddress::try_from(
					hex::decode("4199b3b56213cd4d852cd85bf0049d2abaed17682d").unwrap(),
				)
				.unwrap()
				.to_evm_address();

				let trx_ingresses = trx_ingress_transactions(
					&retry_client,
					HashSet::from([vault_address]),
					block_num,
					block_hash,
				)
				.await
				.unwrap();

				assert_eq!(
					trx_ingresses,
					vec![(
						vault_address,
						2,
						"011fc77de4dd7777d1ddaa5d5411b28c250000631f8aeda0c5808d0d5134e4ca"
							.parse()
							.unwrap(),
					),]
				);

				// Test with vault address that has negative change (should be skipped)
				let vault_address = TronAddress::try_from(
					hex::decode("4104c5b113f9b4d5c836b03adcaec583be67876076").unwrap(),
				)
				.unwrap()
				.to_evm_address();

				let trx_ingresses = trx_ingress_transactions(
					&retry_client,
					HashSet::from([vault_address]),
					block_num,
					block_hash,
				)
				.await
				.unwrap();

				assert_eq!(trx_ingresses, vec![]);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_ingress_vault_swap_trx_decode() {
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
				let block_hash: H256 =
					"0000000003dd6e006934d46981dab0f3cf1863b6d7b0a50577e198e06bb8560b"
						.parse()
						.unwrap();

				// Test vault address
				let vault_address = TronAddress::try_from(
					hex::decode("41c34856cadd5524892907d8a34126053447740375").unwrap(),
				)
				.unwrap()
				.to_evm_address();

				let trx_ingresses = trx_ingress_transactions(
					&retry_client,
					HashSet::from([vault_address]),
					block_num,
					block_hash,
				)
				.await
				.unwrap();

				// Update assertions based on expected results
				assert_eq!(
					trx_ingresses,
					vec![(
						vault_address,
						1000000,
						"f3de44cca0c78890854a637c215e19490211c0ece6cf892fe759773b98dbf900"
							.parse()
							.unwrap(),
					),]
				);

				// This cointains a TRX Vault Swap with valid data
				// https://nile.tronscan.org/#/transaction/b8042280e6a813d65ad01a0555e1e9a9497bf69d012b58cdc5d925c21df35972
				let block_num: u64 = 64845362;
				let trx_ingresses = trx_ingress_transactions(
					&retry_client,
					HashSet::from([vault_address]),
					block_num,
					"0000000003dd7632dd9fcdfcbe8008f7e534191ff5d1ceedb05ac5affdf76b32"
						.parse()
						.unwrap(),
				)
				.await
				.unwrap();

				let ingresses: Vec<(TronAsset, u64, H256)> = trx_ingresses
					.into_iter()
					.map(|(_addr, amount, tx_id)| (TronAsset::Trx, amount, tx_id))
					.collect();
				let vault_swaps = fetch_and_decode_transactions(
					&retry_client,
					ingresses,
					block_num,
					vault_address,
					&HashMap::new(),
				)
				.await
				.into_iter()
				.collect::<Result<Vec<_>, _>>()?;

				let expected_refund_params = ChannelRefundParameters {
					retry_duration: 100,
					refund_address: H160::from_slice(
						&hex::decode("f627b6285759e4fa9ca1214c31f6748afaad766c").unwrap(),
					),
					min_price: cf_amm::math::Price::from_raw(sp_core::U256::from(
						999649550997842449747136364u128,
					)),
					refund_ccm_metadata: None::<Option<CcmChannelMetadata<CcmAdditionalData>>>,
					max_oracle_price_slippage: Some(110),
				};
				use sp_runtime::AccountId32;
				let expected_broker_fee = cf_primitives::Beneficiary {
					account: AccountId32::from([
						0x70, 0xd0, 0xcd, 0x75, 0xa3, 0x67, 0x98, 0x73, 0x44, 0xa3, 0x89, 0x6a,
						0x18, 0xe1, 0x51, 0x0e, 0x54, 0x29, 0xca, 0x5e, 0x88, 0x35, 0x7b, 0x6c,
						0x2a, 0x2e, 0x30, 0x6b, 0x38, 0x77, 0x38, 0x0d,
					]),
					bps: 0,
				};
				let expected_vault_swap_params = VaultSwapParameters {
					refund_params: expected_refund_params,
					dca_params: None,
					boost_fee: 0,
					broker_fee: expected_broker_fee,
					affiliate_fees: Default::default(),
				};
				let expected_tx_id = H256::from_slice(
					&hex::decode(
						"b8042280e6a813d65ad01a0555e1e9a9497bf69d012b58cdc5d925c21df35972",
					)
					.unwrap(),
				);

				// Validate returned vault witness fields we can deterministically assert
				assert_eq!(vault_swaps.len(), 1);
				let returned = &vault_swaps[0];
				assert_eq!(returned.deposit_amount, 1000000u128);
				assert_eq!(returned.tx_id, expected_tx_id);
				// Refund params
				assert_eq!(
					returned.refund_params.retry_duration,
					expected_vault_swap_params.refund_params.retry_duration
				);
				assert_eq!(
					returned.refund_params.max_oracle_price_slippage,
					expected_vault_swap_params.refund_params.max_oracle_price_slippage
				);
				// Broker fee bps and account
				assert_eq!(
					returned.broker_fee.as_ref().unwrap().bps,
					expected_vault_swap_params.broker_fee.bps
				);
				assert_eq!(
					returned.broker_fee.as_ref().unwrap().account,
					expected_vault_swap_params.broker_fee.account
				);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	const VAULT: &str = "32e07e5dfcb75c2977adddc593eb8d6b6e71f96e";
	const USDT: &str = "a614f803b6fd780986a42c78ec9c7f77e6ded13c";

	fn trigger_smart_contract(contract_address: &str, data: &str) -> serde_json::Value {
		serde_json::json!({
			"parameter": {
				"value": {
					"owner_address": "41e092bce6dc52c38fbf7441edc8ea5b89b48a8a5a",
					"contract_address": format!("41{contract_address}"),
					"data": data,
				},
				"type_url": "type.googleapis.com/protocol.TriggerSmartContract"
			},
			"type": "TriggerSmartContract"
		})
	}

	fn trc20_transfer_call(to: &str, amount: u64) -> String {
		format!("{}{:0>64}{:0>64x}", hex::encode(TRC20_TRANSFER_SELECTOR), to, amount)
	}

	fn check(
		contracts: &[serde_json::Value],
		asset: TronAsset,
		amount: u64,
	) -> Result<(), &'static str> {
		ensure_direct_transfer_to_vault(
			contracts,
			asset,
			amount,
			VAULT.parse().unwrap(),
			&HashMap::from([(TronAsset::TrxUsdt, USDT.parse().unwrap())]),
		)
	}

	#[test]
	fn accepts_direct_trx_transfer() {
		let contracts = [serde_json::json!({
			"parameter": {
				"value": {
					"amount": 1000000000u64,
					"owner_address": "4177743208ce9708cce78592dc8677ef3a3cef3490",
					"to_address": format!("41{VAULT}"),
				},
				"type_url": "type.googleapis.com/protocol.TransferContract"
			},
			"type": "TransferContract"
		})];
		assert_eq!(check(&contracts, TronAsset::Trx, 1000000000), Ok(()));
		assert!(check(&contracts, TronAsset::Trx, 999999999).is_err());
		assert!(check(&contracts, TronAsset::TrxUsdt, 1000000000).is_err());
	}

	#[test]
	fn accepts_direct_trc20_transfer() {
		let contracts = [trigger_smart_contract(USDT, &trc20_transfer_call(VAULT, 13000))];
		assert_eq!(check(&contracts, TronAsset::TrxUsdt, 13000), Ok(()));
		assert!(check(&contracts, TronAsset::TrxUsdt, 13001).is_err());
		assert!(check(&contracts, TronAsset::Trx, 13000).is_err());

		// Tron's own encoding keeps the 0x41 address prefix inside the padding.
		let contracts =
			[trigger_smart_contract(USDT, &trc20_transfer_call(&format!("41{VAULT}"), 13000))];
		assert_eq!(check(&contracts, TronAsset::TrxUsdt, 13000), Ok(()));

		// Any other non-zero padding is rejected.
		for prefix in ["01", "ff", "4100"] {
			let contracts = [trigger_smart_contract(
				USDT,
				&trc20_transfer_call(&format!("{prefix}{VAULT}"), 13000),
			)];
			assert_eq!(
				check(&contracts, TronAsset::TrxUsdt, 13000),
				Err("malformed address word"),
				"prefix {prefix}"
			);
		}

		// Transfer to somewhere other than the vault.
		let contracts = [trigger_smart_contract(
			USDT,
			&trc20_transfer_call("e092bce6dc52c38fbf7441edc8ea5b89b48a8a5a", 13000),
		)];
		assert!(check(&contracts, TronAsset::TrxUsdt, 13000).is_err());

		// Transfer call to a contract that is not the token.
		let contracts = [trigger_smart_contract(VAULT, &trc20_transfer_call(VAULT, 13000))];
		assert!(check(&contracts, TronAsset::TrxUsdt, 13000).is_err());
	}

	#[test]
	fn rejects_rebroadcast_vault_fetch() {
		// A re-broadcast of one of our own `fetch` calls, which moves funds from a deposit channel
		// into the vault, with a forged memo attached by the broadcaster.
		let contracts = [trigger_smart_contract(
			VAULT,
			"5f8c0f9ad368062cba9fa834848601526b8a12a5a6b3e6ae7d1bf86c7f45960854d8646e\
			00000000000000000000000000000000000000000000000000000000000014ee\
			000000000000000000000000e8ae3af2abe83aef5821d525344dea99f90c9273\
			00000000000000000000000000000000000000000000000000000000000000c0\
			00000000000000000000000000000000000000000000000000000000000000e0\
			0000000000000000000000000000000000000000000000000000000000000140\
			0000000000000000000000000000000000000000000000000000000000000000\
			0000000000000000000000000000000000000000000000000000000000000001\
			000000000000000000000000bb215fb95320dde10b544a1505bc30acd1e7653c\
			000000000000000000000000a614f803b6fd780986a42c78ec9c7f77e6ded13c\
			0000000000000000000000000000000000000000000000000000000000000000",
		)];
		assert_eq!(
			check(&contracts, TronAsset::TrxUsdt, 12995684310),
			Err("call target is not the token contract")
		);
		assert_eq!(check(&contracts, TronAsset::Trx, 1), Err("unsupported token"));
	}

	#[test]
	fn rejects_multi_contract_transactions() {
		let mut contracts = vec![trigger_smart_contract(USDT, &trc20_transfer_call(VAULT, 1))];
		contracts.push(contracts[0].clone());
		assert_eq!(check(&contracts, TronAsset::TrxUsdt, 1), Err("expected exactly one contract"));
	}

	#[test]
	fn test_encode_tron_vault_swap_data() {
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
	}
}
