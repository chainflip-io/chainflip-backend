use crate::witness::common::{RuntimeCallHasChain, RuntimeHasChain};
use anyhow::{ensure, Error};
use cf_chains::{
	instances::ChainInstanceFor,
	sol::{SolAddress, SolHash, SolSignature},
	Chain, Solana,
};
use cf_primitives::EpochIndex;
use futures_core::Future;

use crate::witness::common::chunked_chain_source::chunked_by_vault::deposit_addresses::Addresses;

use super::super::common::chunked_chain_source::chunked_by_vault::{
	builder::ChunkedByVaultBuilder, ChunkedByVault,
};
use crate::{
	sol::{
		commitment_config::CommitmentConfig,
		retry_rpc::SolRetryRpcApi,
		rpc_client_api::{
			ParsedAccount, Response, RpcAccountInfoConfig, UiAccount, UiAccountData,
			UiAccountEncoding,
		},
	},
	// witness::common::chain_source::Header,
};
use serde_json::Value;
use std::str::FromStr;

// TODO: Get this from some
const FETCH_ACCOUNT_BYTE_LENGTH: usize = 24;
const MAX_MULTIPLE_ACCOUNTS_QUERY: usize = 100;
const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";
pub const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	/// TODO: Add description
	pub async fn solana_deposits<ProcessCall, ProcessingFut, SolRetryRpcClient>(
		self,
		process_call: ProcessCall,
		sol_rpc: SolRetryRpcClient,
		asset: <Inner::Chain as cf_chains::Chain>::ChainAsset,
		vault_address: SolAddress,
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
		self.then(move |epoch, header| {
			assert!(<Inner::Chain as Chain>::is_block_witness_root(header.index));

			let sol_rpc = sol_rpc.clone();
			let process_call = process_call.clone();
			async move {
				let (_, deposit_channels) = header.data;

				// Genesis block cannot contain any transactions
				if !deposit_channels.is_empty() {
					// TODO: Handle how to split/chain deposit channels and fetch accounts
					// For now we assume they are properly ordered alternated
					let addresses = deposit_channels
						.into_iter()
						// TODO For the sol_account_infos we won't need to split for assets
						// but we might have to do it to fit into the architecture of the CFE.
						// E.g. We need to submit the asset on the cf_ingress_egress so effectively
						// the submissions need to be separate
						// .filter(|deposit_channel| deposit_channel.deposit_channel.asset == asset)
						// TODO: Here we can maybe get the fetch account (if part of the
						// DepositChannel) struct in the DepositChannelState field. Maybe also keep
						// the asset on a per deposit channel so we can use the same RPC call.
						.map(|deposit_channel| deposit_channel.deposit_channel.address)
						.collect::<Vec<_>>();

					let chunked_addresses: Vec<Vec<_>> = addresses
						.chunks(MAX_MULTIPLE_ACCOUNTS_QUERY)
						.map(|chunk| chunk.to_vec())
						.collect();

					// TODO: Check with Alastair what we need to submit. At least we need to check
					// the deposit account and the fetch account and put them into one submission.
					for chunk_address in chunked_addresses {
						let account_infos =
							sol_account_infos(&sol_rpc, chunk_address, vault_address).await?;
						let slot = account_infos.1;
						let ingresses = sol_ingresses(account_infos.0)?;

						// Is this to not submit non-changes?
						if !ingresses.is_empty() {
							process_call(
									pallet_cf_ingress_egress::Call::<
										_,
										ChainInstanceFor<Inner::Chain>,
									>::process_deposits {
										deposit_witnesses: ingresses
											.into_iter()
											.map(|(to_addr, value)| {
												pallet_cf_ingress_egress::DepositWitness {
													deposit_address: to_addr,
													asset,
													amount:
														value
														.try_into()
														.expect("Ingress witness transfer value should fit u128"),
													deposit_details: (),
												}
											})
											.collect(),
										block_height: slot,
									}
									.into(),
									epoch.index,
								)
								.await;
						}
					}
				}
				Ok::<_, anyhow::Error>(())
			}
		})
	}
}

async fn sol_account_infos<SolRetryRpcClient>(
	sol_rpc: &SolRetryRpcClient,
	addresses: Vec<SolAddress>,
	vault_address: SolAddress,
) -> Result<(Vec<(SolAddress, u128)>, u64), anyhow::Error>
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

	let system_program_pubkey = SolAddress::from_str(SYSTEM_PROGRAM_ID).unwrap();
	let associated_token_account_pubkey = SolAddress::from_str(TOKEN_PROGRAM_ID).unwrap();

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

					let owner_pub_key = SolAddress::from_str(account_info.owner.as_str()).unwrap();
					println!("owner_pub_key {:?}", owner_pub_key);
					let amount = if owner_pub_key == system_program_pubkey {
						// Native deposit channel
						// TODO: Add a check for base64 encoding with empty data
						println!("Native deposit channel found");
						Ok(account_info.lamports as u128)
					// Fetch account. We either get the Vault address or we default to it
					// if we trust it's correctness
					} else if owner_pub_key == vault_address {
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

								// Print the bytes that will be removed
								println!("bytes to be removed {:?}", &bytes[..8]);

								// Check that there are 24 bytes (16 from u128 + 8 from
								// discriminator)
								ensure!(bytes.len() == FETCH_ACCOUNT_BYTE_LENGTH);

								// Remove the discriminator
								// TODO: Check that we are removing the correct ones. We could even
								// have a check that the discriminator is the correct one.
								bytes.drain(..8);

								let array: [u8; 16] =
									bytes.try_into().expect("Byte slice length doesn't match u128");

								// TODO: Check that this conversion works with the real contract
								let fetch_cumulative = u128::from_le_bytes(array);

								Ok(fetch_cumulative)
							},
							_ => Err(anyhow::anyhow!("Unexpected fetch account encoding")),
						}
					} else if owner_pub_key == associated_token_account_pubkey {
						// Token deposit channel
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

								amount_str
									.parse()
									.map_err(|_| anyhow::anyhow!("Failed to parse string to u128"))
							},
							_ => Err(anyhow::anyhow!("Unexpected token account encoding")),
						}
					} else {
						Err(anyhow::anyhow!("Unexpected account - unexpected owner"))
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

// TODO: For now we just presupose that the accounts are two consecutive items
fn sol_ingresses(
	account_infos: Vec<(SolAddress, u128)>,
) -> Result<Vec<(SolAddress, u128)>, anyhow::Error> {
	if account_infos.len() % 2 != 0 {
		return Err(anyhow::anyhow!("The number of items in the vector must be even"));
	}

	let result = account_infos
		.chunks(2)
		.map(|account_pair| {
			let (deposit_channel_address, value1) = account_pair[0];
			let (_, value2) = account_pair[1];
			// It should never reach saturation => (2^128 - 1) // 10 ^ 9
			(deposit_channel_address, value1.saturating_add(value2))
		})
		.collect::<Vec<(_, _)>>();

	Ok(result)
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{NodeContainer, WsHttpEndpoints},
		// use settings:: Settings
		sol::{
			retry_rpc::SolRetryRpcClient,
			// retry_rpc::SolRetryRpcApi
			// rpc::SolRpcClient,
		},
		witness::sol::sol_deposits::sol_account_infos,
		// witness::common::chain_source::Header
	};

	use cf_chains::{sol::SolAddress, Chain, Solana};
	use futures_util::FutureExt;
	use std::str::FromStr;
	use utilities::task_scope;

	#[test]
	fn test_sol_ingresses() {
		let account_infos = vec![
			(SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ").unwrap(), 1),
			(SolAddress::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz").unwrap(), 2),
			(SolAddress::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt").unwrap(), 3),
			(SolAddress::from_str("ELF78ZhSr8u4SCixA7YSpjdZHZoSNrAhcyysbavpC2kA").unwrap(), 4),
		];

		let ingresses = super::sol_ingresses(account_infos.clone()).unwrap();

		assert_eq!(ingresses.len(), 2);
		assert_eq!(ingresses[0].0, account_infos[0].0);
		assert_eq!(ingresses[0].1, 3);
		assert_eq!(ingresses[1].0, account_infos[2].0);
		assert_eq!(ingresses[1].1, 7);
	}

	#[test]
	fn test_sol_ingresses_error_if_odd_length() {
		let account_infos = vec![
			(SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ").unwrap(), 1),
			(SolAddress::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz").unwrap(), 2),
			(SolAddress::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt").unwrap(), 3),
		];

		let ingresses = super::sol_ingresses(account_infos);

		assert!(ingresses.is_err());
	}

	// TODO: Add test for Fetch Account from a live network (deploy it there)

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

				let owner_account =
					SolAddress::from_str("gSbePebfvPy7tRqimPoVecS2UsBvYv46ynrzWocc92s").unwrap();

				let mut addresses = vec![
					// Normal account owned by system program - should be understood as a deposit
					// channel
					SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ").unwrap(),
					// Token account - should be understood as a fetch account
					SolAddress::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz").unwrap(),
					// Empty account - should return zero amount (non initialized fetch/deposit
					// channel)
					SolAddress::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt").unwrap(),
				];

				let account_infos: (Vec<(SolAddress, u128)>, u64) =
					sol_account_infos(&retry_client, addresses.clone(), owner_account)
						.await
						.unwrap();
				println!("Result {:?}", account_infos);

				// Try an account with data that is not of the same length
				addresses.push(
					SolAddress::from_str("ELF78ZhSr8u4SCixA7YSpjdZHZoSNrAhcyysbavpC2kA").unwrap(),
				);
				let account_infos =
					sol_account_infos(&retry_client, addresses, owner_account).await;
				assert!(account_infos.is_err());

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
