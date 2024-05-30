use crate::witness::common::{RuntimeCallHasChain, RuntimeHasChain};
use anyhow::{ensure, Error};
use cf_chains::{
	instances::ChainInstanceFor,
	sol::{SolAddress, SolHash},
	Chain,
};
use cf_primitives::EpochIndex;
use futures_core::Future;
use sol_prim::pda::derive_associated_token_account;

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
pub use sol_prim::{
	consts::{SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
	pda::derive_fetch_account,
};
use std::str::FromStr;

use crate::common::Mutex;
use std::{collections::HashMap, sync::Arc};

const FETCH_ACCOUNT_BYTE_LENGTH: usize = 24;
const MAX_MULTIPLE_ACCOUNTS_QUERY: usize = 100;
const FETCH_ACCOUNT_DISCRIMINATOR: [u8; 8] = [188, 68, 197, 38, 48, 192, 81, 100];

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	/// TODO: Add description
	pub async fn solana_deposits<ProcessCall, ProcessingFut, SolRetryRpcClient>(
		self,
		process_call: ProcessCall,
		sol_rpc: SolRetryRpcClient,
		asset: <Inner::Chain as cf_chains::Chain>::ChainAsset,
		vault_address: SolAddress,
		token_pubkey: Option<SolAddress>,
		cached_balances: Arc<Mutex<HashMap<SolAddress, u128>>>,
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
		println!("DEBUGDEPOSITS Processing Solana Deposits");

		self.then(move |epoch, header| {
			assert!(<Inner::Chain as Chain>::is_block_witness_root(header.index));

			let sol_rpc = sol_rpc.clone();
			let process_call = process_call.clone();
			let cached_balances = Arc::clone(&cached_balances);

			println!("DEBUGDEPOSITS Processing Solana Deposits Inner");

			async move {
				let (_, deposit_channels) = header.data;

				// TODO: Use DB instead?
				// Using this as a global variable to store the previous balances. The new pallet it
				// might be alright if we submit values again after a restart, it's just not
				// let cached_balances = Arc::new(Mutex::new(HashMap::new()));

				println!("DEBUGDEPOSITS Processing Solana Deposits Inner 2");

				// TODO: Check that if asset != sol (native) then token_pubkey is Some??

				// Genesis block cannot contain any transactions
				if !deposit_channels.is_empty() {
					let deposit_channels_info = deposit_channels
						.into_iter()
						// TODO Consider not splitting this to reuse the same get_multiple_account
						// rpc call. Doing it now because when we submit the vector as an extrinsic
						// it gets submitted on a per asset basis
						.filter(|deposit_channel| deposit_channel.deposit_channel.asset == asset)
						.map(|deposit_channel| {
							(
								deposit_channel.deposit_channel.address,
								derive_fetch_account(
									vault_address,
									deposit_channel.deposit_channel.address,
								)
								.expect("Failed to derive fetch account"),
							)
						})
						.collect::<Vec<_>>();

					println!(
						"DEBUGDEPOSITS Processing Solana Deposits {:?}",
						deposit_channels_info
					);

					let chunked_deposit_channels_info: Vec<Vec<(SolAddress, SolAddress)>> =
						deposit_channels_info
							.chunks(MAX_MULTIPLE_ACCOUNTS_QUERY / 2)
							.map(|chunk| chunk.to_vec())
							.collect();

					// TODO: Should submit every deposit channel separately with the new pallet?
					for deposit_channels_info in chunked_deposit_channels_info {
						let ingresses = ingress_amounts(
							&sol_rpc,
							deposit_channels_info,
							vault_address,
							token_pubkey,
						)
						.await?;

						if !ingresses.0.is_empty() {
							let block_height = ingresses.1;

							let mut cached_balances = cached_balances.lock().await;

							// Filter out the deposit channels that have the same balance
							let ingresses: Vec<(SolAddress, u128)> = ingresses
								.0
								.into_iter()
								.filter_map(|(deposit_channel_address, value)| {
									let deposit_channel_cached_balance =
										cached_balances.get(&deposit_channel_address).unwrap_or(&0);

									println!(
										"DEBUGDEPOSITS deposit_channel_address {:?}, cached_balance {:?}, value {:?}, ",
										deposit_channel_address,
										*deposit_channel_cached_balance,
										value
									);

									if value > *deposit_channel_cached_balance {
										// TODO: We should submit the value as is with the new
										// pallet Some((deposit_channel_address, value))
										Some((
											deposit_channel_address,
											value - deposit_channel_cached_balance,
										))
									} else {
										None
									}
								})
								.collect::<Vec<_>>();

							println!("DEBUGDEPOSITS Submitting ingresses {:?}", ingresses);
							process_call(
								pallet_cf_ingress_egress::Call::<_, ChainInstanceFor<Inner::Chain>>::process_deposits {
									deposit_witnesses: ingresses.clone()
										.into_iter()
										.map(|(deposit_channel_address, value)| {
											pallet_cf_ingress_egress::DepositWitness {
												deposit_address: deposit_channel_address,
												asset,
												amount: value
													.try_into()
													.expect("Ingress witness transfer value should fit u128"),
												deposit_details: (),
											}
										})
										.collect(),
									block_height,
								}
								.into(),
								epoch.index,
							)
							.await;

							// Update hashmap
							ingresses.into_iter().for_each(|(deposit_channel_address, value)| {
								println!(
									"DEBUGDEPOSITS Updating cached_balances for {:?} to value {:?}",
									deposit_channel_address, value
								);
								cached_balances.insert(deposit_channel_address, value);
							});
						}
					}
				}
				Ok::<_, anyhow::Error>(())
			}
		})
	}
}

// We might end up splitting this into two functions, one for native deposit channels and one for
// tokens to make it simpler For now this will expect all deposit channels to be either native or
// token
async fn ingress_amounts<SolRetryRpcClient>(
	sol_rpc: &SolRetryRpcClient,
	deposit_channels_info: Vec<(SolAddress, SolAddress)>,
	vault_address: SolAddress,
	token_pubkey: Option<SolAddress>,
) -> Result<(Vec<(SolAddress, u128)>, u64), anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let system_program_pubkey = SolAddress::from_str(SYSTEM_PROGRAM_ID).unwrap();
	let associated_token_account_pubkey = SolAddress::from_str(TOKEN_PROGRAM_ID).unwrap();

	// Ensure that if !is_native_asset then ther eis a value in token_pubkey
	let is_native_asset = token_pubkey.is_none();

	// Flattening it to have both deposit channels and fetch accounts
	let addresses_to_witness = deposit_channels_info
		.iter()
		.flat_map(|(address, b)| {
			vec![
				if is_native_asset {
					*address
				} else {
					// Checked above that it's Some()
					derive_associated_token_account(*address, token_pubkey.unwrap())
						.expect("Failed to derive associated token account")
						.0
				},
				*b,
			]
		})
		.collect::<Vec<_>>();

	ensure!(addresses_to_witness.len() <= MAX_MULTIPLE_ACCOUNTS_QUERY);

	let accounts_info: Response<Vec<Option<UiAccount>>> = sol_rpc
		.get_multiple_accounts(
			addresses_to_witness.as_slice(),
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

	ensure!(deposit_channels_info.len() * 2 == accounts_info.value.len());

	// For now we infer the address type from the owner. However, we might want to enforce
	// from the ordering (they should be alternate) and/or whether the deposit channels are
	// SOL/Tokens.
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
						ensure!(is_native_asset);
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
								ensure!(encoding == UiAccountEncoding::Base64);

								// Decode the base64 string to bytes
								let mut bytes = base64::decode(base64_string)
									.expect("Failed to decode base64 string");

								// Check that there are 24 bytes (16 (u128) + 8 (discriminator))
								ensure!(bytes.len() == FETCH_ACCOUNT_BYTE_LENGTH);

								// Remove the discriminator
								let discriminator: Vec<u8> = bytes.drain(..8).collect();

								ensure!(
									discriminator == FETCH_ACCOUNT_DISCRIMINATOR,
									"Discriminator does not match expected value"
								);

								let array: [u8; 16] =
									bytes.try_into().expect("Byte slice length doesn't match u128");

								let fetch_cumulative = u128::from_le_bytes(array);

								Ok(fetch_cumulative)
							},
							_ => Err(anyhow::anyhow!("Unexpected fetch account encoding")),
						}
					} else if owner_pub_key == associated_token_account_pubkey {
						// Token deposit channel
						println!("Associated token account");
						let original_deposit_channel = deposit_channels_info[index / 2].0;
						ensure!(is_native_asset == false);

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

								// Checking mint pubkey and owner. Might not be necessary
								let owner = info
									.get("owner")
									.and_then(|v| v.as_str())
									.ok_or(anyhow::anyhow!("Missing 'owner' field"))?;

								ensure!(owner == original_deposit_channel.to_string());

								let mint = info
									.get("mint")
									.and_then(|v| v.as_str())
									.ok_or(anyhow::anyhow!("Missing 'mint' field"))?;

								// Checked before that it's Some() so we could use unwrap
								let token_pubkey_str = token_pubkey
									.map(|tpk| tpk.to_string())
									.ok_or(anyhow::anyhow!("token_pubkey is None"))?;

								ensure!(mint == token_pubkey_str);

								let amount = info
									.get("tokenAmount")
									.and_then(|token_amount| token_amount.get("amount"))
									.and_then(|v| v.as_str())
									.ok_or(anyhow::anyhow!(
										"Missing 'tokenAmount' or 'amount' field"
									))?
									.parse()
									.map_err(|_| anyhow::anyhow!("Failed to parse string to u128"));

								amount
							},
							_ => Err(anyhow::anyhow!("Unexpected token account encoding")),
						}
					} else {
						Err(anyhow::anyhow!("Unexpected account - unexpected owner"))
					}?;
					Ok((deposit_channels_info[index / 2].0, amount))
				},
				// When no account in the address
				None => {
					println!("Empty account found");
					Ok((deposit_channels_info[index / 2].0, 0_u128))
				},
			}
		})
		.collect::<Result<Vec<(_, _)>, Error>>()?;

	// Now we will have Vec<(SolAddress, fetch/balance), slot> . We need to process that to return a
	// valid list
	Ok((sol_ingresses(accounts_info).expect("Failed to calculate ingresses"), slot))
}

// Accounts and fetch accounts are supposed to be in consecutive order
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

	ensure!(result.len() == account_infos.len() / 2);

	Ok(result)
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{NodeContainer, WsHttpEndpoints},
		// use settings:: Settings
		sol::retry_rpc::SolRetryRpcClient,
	};

	use cf_chains::{sol::SolAddress, Chain, Solana};
	use futures_util::FutureExt;
	use std::str::FromStr;
	use utilities::task_scope;

	use super::*;

	#[test]
	fn test_sol_ingresses() {
		let account_infos = vec![
			(SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ").unwrap(), 1),
			(SolAddress::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz").unwrap(), 2),
			(SolAddress::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt").unwrap(), 3),
			(SolAddress::from_str("ELF78ZhSr8u4SCixA7YSpjdZHZoSNrAhcyysbavpC2kA").unwrap(), 4),
		];

		let ingresses = sol_ingresses(account_infos.clone()).unwrap();

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

		let ingresses = sol_ingresses(account_infos);

		assert!(ingresses.is_err());
	}

	#[test]
	fn test_sol_derive_fetch_account() {
		let fetch_account = derive_fetch_account(
			SolAddress::from_str("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf").unwrap(),
			SolAddress::from_str("HAMxiXdEJxiBHabZAUm8PSLvWQM2GHi5PArVZvUCeDab").unwrap(),
		)
		.unwrap();
		assert_eq!(
			fetch_account,
			SolAddress::from_str("HGgUaHpsmZpB3pcYt8PE89imca6BQBRqYtbVQQqsso3o").unwrap()
		);
	}

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

				let vault_account =
					SolAddress::from_str("gSbePebfvPy7tRqimPoVecS2UsBvYv46ynrzWocc92s").unwrap();
				let vault_program =
					SolAddress::from_str("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf").unwrap();

				let native_deposit_channel =
					SolAddress::from_str("ssvtUfHGexqLHjuNf6ngQScL2nFp79ergVQTmGAoHCA").unwrap();
				let fetch_account_0 =
					derive_fetch_account(vault_program, native_deposit_channel).unwrap();

				let empty_account =
					SolAddress::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt").unwrap();
				let fetch_account_1 = derive_fetch_account(vault_program, empty_account).unwrap();

				let mut addresses = vec![
					(native_deposit_channel, fetch_account_0),
					(empty_account, fetch_account_1),
				];

				let account_infos: (Vec<(SolAddress, u128)>, u64) =
					ingress_amounts(&retry_client, addresses.clone(), vault_account, None)
						.await
						.unwrap();
				println!("Result Native {:?}", account_infos);
				assert_eq!(account_infos.0[0], (native_deposit_channel, 5000000000));
				assert_eq!(account_infos.0[1], (empty_account, 0));

				// Derived ATA will be 5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz
				let token_deposit_channel =
					SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ").unwrap();
				let deposit_channel_ata =
					SolAddress::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz").unwrap();

				// Passing directly token account without a mint should fail
				let mut new_addresses = addresses.clone();
				new_addresses.push((deposit_channel_ata, empty_account));
				assert!(
					ingress_amounts(&retry_client, new_addresses.clone(), vault_account, None,)
						.await
						.is_err()
				);

				let fetch_account_2 =
					derive_fetch_account(vault_program, token_deposit_channel).unwrap();
				let token_mint_pubkey =
					SolAddress::from_str("MAZEnmTmMsrjcoD6vymnSoZjzGF7i7Lvr2EXjffCiUo").unwrap();

				let new_addresses = vec![(token_deposit_channel, fetch_account_2)];

				let account_infos: (Vec<(SolAddress, u128)>, u64) = ingress_amounts(
					&retry_client,
					new_addresses.clone(),
					vault_account,
					Some(token_mint_pubkey),
				)
				.await
				.unwrap();
				assert_eq!(account_infos.0[0], (token_deposit_channel, 10000));

				// Try an account with data that is not of the same length
				let fetch_account_3: SolAddress =
					SolAddress::from_str("ELF78ZhSr8u4SCixA7YSpjdZHZoSNrAhcyysbavpC2kA").unwrap();

				let mut new_addresses = addresses.clone();
				new_addresses.push((empty_account, fetch_account_3));
				let account_infos = ingress_amounts(
					&retry_client,
					new_addresses,
					vault_account,
					Some(token_mint_pubkey),
				)
				.await;
				assert!(account_infos.is_err());

				// Try real fetch data account
				let real_fetch_data_account =
					SolAddress::from_str("HsRFLNzidLJx4RuqxdT924btgCTTuFFmNqp7Ph9y9HdN").unwrap();
				addresses.push((empty_account, real_fetch_data_account));

				let account_infos = ingress_amounts(
					&retry_client,
					addresses.clone(),
					SolAddress::from_str("EVo1QjbAPKs4UbS78uNk2LpNG7hAWJum52ybQMtxgVL2").unwrap(),
					Some(token_mint_pubkey),
				)
				.await
				.unwrap();
				assert_eq!(account_infos.0[2], (empty_account, 74510874563096));

				// Wrong vault address (owner) when checking ownership of a fetch account
				assert!(ingress_amounts(
					&retry_client,
					addresses.clone(),
					SolAddress::from_str("So11111111111111111111111111111111111111112").unwrap(),
					Some(token_mint_pubkey),
				)
				.await
				.is_err());

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
