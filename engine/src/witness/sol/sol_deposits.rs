use crate::witness::common::{RuntimeCallHasChain, RuntimeHasChain};
use anyhow::{ensure, Error};
use cf_chains::{
	instances::ChainInstanceFor,
	sol::{SolAddress, SolHash, SolPubkey},
	Chain,
};
use cf_primitives::EpochIndex;
use futures_core::Future;
use sp_core::{H160, H256};

use crate::witness::common::chunked_chain_source::chunked_by_vault::deposit_addresses::Addresses;

use std::{any, collections::BTreeMap};

use itertools::Itertools;

use crate::witness::common::chain_source::Header;

use super::super::common::chunked_chain_source::chunked_by_vault::{
	builder::ChunkedByVaultBuilder, ChunkedByVault,
};
use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::SolRetryRpcApi,
	rpc_client_api::{
		ParsedAccount, Response, RpcAccountInfoConfig, UiAccount, UiAccountData, UiAccountEncoding,
	},
};
use serde_json::Value;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	/// TODO: Add description
	pub async fn solana_deposits<ProcessCall, ProcessingFut, SolRetryRpcClient>(
		self,
		process_call: ProcessCall,
		sol_rpc: SolRetryRpcClient,
		asset: <Inner::Chain as cf_chains::Chain>::ChainAsset,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner::Chain:
			cf_chains::Chain<ChainAmount = u64, DepositDetails = (), ChainAccount = SolAddress>,
		Inner: ChunkedByVault<Index = u64, Hash = SolHash, Data = ((), Addresses<Inner>)>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
		SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		self.then(move |_epoch, header| {
			assert!(<Inner::Chain as Chain>::is_block_witness_root(header.index));

			let sol_rpc = sol_rpc.clone();
			let process_call = process_call.clone();
			async move {
				let (_, deposit_channels) = header.data;

				// Genesis block cannot contain any transactions
				if !deposit_channels.is_empty() {
					let addresses = deposit_channels
						.into_iter()
						.filter(|deposit_channel| deposit_channel.deposit_channel.asset == asset)
						.map(|deposit_channel| deposit_channel.deposit_channel.address)
						.collect::<Vec<_>>();

					// Do a match statement for USDC

					// let ingresses = sol_ingresses_at_block(
					// 		&sol_rpc,
					// 		addresses,
					// 	)
					// 	.await?;

					// 	events_at_block::<Inner::Chain, VaultEvents, _>(
					// 		Header {
					// 			index: header.index,
					// 			hash: header.hash,
					// 			parent_hash: header.parent_hash,
					// 			data: bloom,
					// 		},
					// 		vault_address,
					// 		&sol_rpc,
					// 	)
					// 	.await?
					// 	.into_iter()
					// 	.filter_map(|event| match event.event_parameters {
					// 		VaultEvents::FetchedNativeFilter(event) => Some(event),
					// 		_ => None,
					// 	})
					// 	.collect(),
					// )?;

					// if !ingresses.is_empty() {
					// 	process_call(
					// 		pallet_cf_ingress_egress::Call::<
					// 			_,
					// 			ChainInstanceFor<Inner::Chain>,
					// 		>::process_deposits {
					// 			deposit_witnesses: ingresses
					// 				.into_iter()
					// 				.map(|(to_addr, value)| {
					// 					pallet_cf_ingress_egress::DepositWitness {
					// 						deposit_address: to_addr,
					// 						asset: asset,
					// 						amount:
					// 							value
					// 							.try_into()
					// 							.expect("Ingress witness transfer value should fit u128"),
					// 						deposit_details: (),
					// 					}
					// 				})
					// 				.collect(),
					// 			block_height: header.index,
					// 		}
					// 		.into(),
					// 		epoch.index,
					// 	)
					// 	.await;
					// }
				}
				Ok::<_, anyhow::Error>(())
			}
		})
	}
}


async fn sol_account_infos<SolRetryRpcClient>(
	sol_rpc: &SolRetryRpcClient,
	addresses: Vec<SolPubkey>,
) -> Result<(Vec<(SolPubkey, u128)>, u64), anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let accounts_info: Response<Vec<Option<UiAccount>>> = sol_rpc
		.get_multiple_accounts_with_config(
			addresses.clone().as_slice(),
			RpcAccountInfoConfig {
				// Using JsonParsed will return the token accounts and the sol deposit channels
				// in a nicely parsed format. For our fetch token accounts it will return it encoded
				// as the default base64.
				encoding: Some(UiAccountEncoding::JsonParsed),
				data_slice: None,
				commitment: Some(CommitmentConfig::finalized()),
				min_context_slot: None,
			},
		)
		.await;

	let slot = accounts_info.context.slot;

	ensure!(addresses.len() == accounts_info.value.len());

	// For now we infer the address type from the owner. However, we might want to enforce
	// the ordering (e.g. deposit channel, fetch account, deposit channel, fetch account,
	// etc.) and/or whether the deposit channels are SOL/Tokens.
	let accounts_info = accounts_info
		.value
		.into_iter()
		.enumerate()
		.map(move |(index, account_info)| {
			match account_info {
				Some(account_info) => {
					println!("Parsing account_info {:?}", account_info);
					let amount = match account_info.owner.as_str() {
						// Native deposit channel
						// TODO: Add a check for base64 encoding with empty data
						"11111111111111111111111111111111" => {
							println!("Native deposit channel found");
							Ok(account_info.lamports as u128)
						},
						// Fetch account. We either get the Vault address or we default to it.
						"THIS_SHOULD_BE_THE_VAULT_ADDRESS" => {
							println!("Vault fetch account found");
							// Fetch data and ensure it's encoding is base64
							match account_info.data {
								// Fetch Data Account
								UiAccountData::Binary(base64_string, encoding) => {
									println!("encoding {:?}", encoding);
									println!("base64_string {:?}", base64_string);

									ensure!(encoding == UiAccountEncoding::Base64);

									// Decode the base64 string to bytes
									let mut bytes = base64::decode(&base64_string)
										.expect("Failed to decode base64 string");

									println!("bytes {:?}", bytes);

									// Check that there are 24 bytes (16 from u128 + 8 from
									// discriminator)
									ensure!(bytes.len() == 24);

									// Remove the discriminator
									// TODO: Check that we are removing the correct ones
									bytes.drain(..8);

									let array: [u8; 16] = bytes
										.try_into()
										.expect("Byte slice length doesn't match u128");

									// TODO: Check that this conversion works with the real contract
									let fetch_cumulative = u128::from_le_bytes(array);

									Ok(fetch_cumulative)
								},
								_ => Err(anyhow::anyhow!("Unexpected fetch account encoding")),
							}
						},
						// Token deposit channel
						"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" => {
							println!("Associated token account");

							// Fetch data and ensure it's encoding is JsonParsed
							match account_info.data {
								UiAccountData::Json(ParsedAccount {
									parsed: Value::Object(json_parsed_account_data),
									..
								}) => {
									let info = json_parsed_account_data
										.get("info")
										.and_then(|v| v.as_object())
										.ok_or(anyhow::anyhow!("Missing 'info' field"))?;

									let token_amount = info
										.get("tokenAmount")
										.ok_or(anyhow::anyhow!("Missing 'tokenAmount' field"))?;

									// TODO: Do we want to check decimals and/or mintpubkey and/or
									// owner?

									let amount_str = token_amount
										.get("amount")
										.and_then(|v| v.as_str())
										.ok_or(anyhow::anyhow!("Missing 'amount' field"))?;

									amount_str.parse().map_err(|_| {
										anyhow::anyhow!("Failed to parse string to u128")
									})
								},
								_ => Err(anyhow::anyhow!("Unexpected token account encoding")),
							}
						},
						_ => Err(anyhow::anyhow!("Unexpected account - unexpected owner")),
					}?;
					Ok((addresses[index], amount))
				},
				// When no account in the address
				None => {
					println!("Empty account found");
					Ok((addresses[index], 0_u128))
				},
			}
		})
		.collect::<Result<Vec<(_, _)>, Error>>()?;

	Ok((accounts_info, slot))
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{NodeContainer, Settings, WsHttpEndpoints},
		sol::{
			retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
			rpc::SolRpcClient,
		},
		witness::{common::chain_source::Header, sol::sol_deposits::sol_account_infos},
	};

	use cf_chains::{sol::SolPubkey, Chain, Solana};
	use futures_util::FutureExt;
	use std::str::FromStr;
	use utilities::task_scope;

	#[tokio::test]
	async fn test_get_deposit_channels_info() {
		task_scope::task_scope(|scope| {
			async {
				// let settings = Settings::new_test().unwrap();
				// let client = SolRetryRpcClient::<SolRpcClient>::new(
				// 	scope,
				// 	settings.sol.nodes,
				// 	U256::from(1337u64),
				// 	"sol_rpc",
				// 	"sol_subscribe",
				// 	"Ethereum",
				// 	Ethereum::WITNESS_PERIOD,
				// )
				// .unwrap();

				let retry_client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: WsHttpEndpoints {
							ws_endpoint: "wss://api.devnet.solana.com".into(),
							http_endpoint: "https://api.devnet.solana.com".into(),
						},
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let mut addresses = vec![
					// Normal account owned by system program - should be understood as a deposit
					// channel
					SolPubkey::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ").unwrap(),
					// Token account - should be understood as a fetch account
					SolPubkey::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz").unwrap(),
					// Empty account - should return zero amount (non initialized fetch/deposit
					// channel)
					SolPubkey::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt").unwrap(),
				];

				let account_infos: (Vec<(SolPubkey, u128)>, u64) =
					sol_account_infos(&retry_client, addresses).await.unwrap();
				println!("Result {:?}", account_infos);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
