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

					let chunked_deposit_channels_info: Vec<Vec<(SolAddress, SolAddress)>> =
						deposit_channels_info
							.chunks(MAX_MULTIPLE_ACCOUNTS_QUERY / 2)
							.map(|chunk| chunk.to_vec())
							.collect();

					// TODO: Check if we should submit every deposit channel separately.
					for deposit_channels_info in chunked_deposit_channels_info {
						let ingresses = ingress_amounts(
							&sol_rpc,
							deposit_channels_info,
							vault_address,
							// TODO: Do it in a nicer way?
							token_pubkey.is_none(),
							token_pubkey,
						)
						.await?;

						// Is this to not submit non-changes?
						if !ingresses.0.is_empty() {
							process_call(
								pallet_cf_ingress_egress::Call::<_, ChainInstanceFor<Inner::Chain>>::process_deposits {
									deposit_witnesses: ingresses.0
										.into_iter()
										.map(|(to_addr, value)| {
											// TODO: Submit the value only if it's different from the previously submitted
											// We should probably store that in db
											pallet_cf_ingress_egress::DepositWitness {
												deposit_address: to_addr,
												asset,
												// TODO: Check with if we should just submit the total number (fetch + balance)
												amount: value
													.try_into()
													.expect("Ingress witness transfer value should fit u128"),
												deposit_details: (),
											}
										})
										.collect(),
									block_height: ingresses.1,
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

// We might end up splitting this into two functions, one for
// native deposit channels and one for tokens to make it simpler
async fn ingress_amounts<SolRetryRpcClient>(
	sol_rpc: &SolRetryRpcClient,
	deposit_channels_info: Vec<(SolAddress, SolAddress)>,
	vault_address: SolAddress,
	is_native_asset: bool,
	token_pubkey: Option<SolAddress>,
) -> Result<(Vec<(SolAddress, u128)>, u64), anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let system_program_pubkey = SolAddress::from_str(SYSTEM_PROGRAM_ID).unwrap();
	let associated_token_account_pubkey = SolAddress::from_str(TOKEN_PROGRAM_ID).unwrap();

	// Ensure that if !is_native_asset then ther eis a value in token_pubkey
	if !is_native_asset {
		ensure!(token_pubkey.is_some());
	}

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
						// TODO: Add a check for base64 encoding with empty data?
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

								println!("bytes {:?}", bytes);

								// Print the bytes that will be removed
								println!("bytes to be removed {:?}", &bytes[..8]);

								// Check that there are 24 bytes (16 from u128 + 8 from
								// discriminator)
								ensure!(bytes.len() == FETCH_ACCOUNT_BYTE_LENGTH);

								// Remove the discriminator
								// TODO: Check that we are removing the correct ones. We could even
								// have a check that the discriminator is the correct one.
								let discriminator: Vec<u8> = bytes.drain(..8).collect();

								ensure!(
									discriminator == FETCH_ACCOUNT_DISCRIMINATOR,
									"Discriminator does not match expected value"
								);

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

								// TODO: Do we to check the mintpubkey and/or owner?

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
					Ok((addresses_to_witness[index], amount))
				},
				// When no account in the address
				None => {
					println!("Empty account found");
					Ok((addresses_to_witness[index], 0_u128))
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

				let owner_account =
					SolAddress::from_str("gSbePebfvPy7tRqimPoVecS2UsBvYv46ynrzWocc92s").unwrap();
				let vault_program =
					SolAddress::from_str("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf").unwrap();

				let native_deposit_channel =
					SolAddress::from_str("ssvtUfHGexqLHjuNf6ngQScL2nFp79ergVQTmGAoHCA").unwrap();
				let fetch_account_0 =
					derive_fetch_account(vault_program, native_deposit_channel).unwrap();

				let empty_account =
					SolAddress::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt").unwrap();
				let fetch_account_2 = derive_fetch_account(vault_program, empty_account).unwrap();

				let mut addresses = vec![
					(native_deposit_channel, fetch_account_0),
					(empty_account, fetch_account_2),
				];

				let account_infos: (Vec<(SolAddress, u128)>, u64) =
					ingress_amounts(&retry_client, addresses.clone(), owner_account, true, None)
						.await
						.unwrap();
				println!("Result Native {:?}", account_infos);
				assert_eq!(account_infos.0[0], (native_deposit_channel, 5000000000));
				assert_eq!(account_infos.0[1], (empty_account, 0));

				let token_deposit_channel =
					SolAddress::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz").unwrap();
				let fetch_account_1 =
					derive_fetch_account(vault_program, token_deposit_channel).unwrap();
				let token_mint_pubkey =
					SolAddress::from_str("MAZEnmTmMsrjcoD6vymnSoZjzGF7i7Lvr2EXjffCiUo").unwrap();

				addresses.push((token_deposit_channel, fetch_account_1));
				// Try an account with data that is not of the same length
				let fetch_account_3: SolAddress =
					SolAddress::from_str("ELF78ZhSr8u4SCixA7YSpjdZHZoSNrAhcyysbavpC2kA").unwrap();
				let account_infos: (Vec<(SolAddress, u128)>, u64) =
					ingress_amounts(&retry_client, addresses.clone(), owner_account, true, None)
						.await
						.unwrap();
				assert_eq!(account_infos.0[0], (native_deposit_channel, 5000000000));
				assert_eq!(account_infos.0[1], (empty_account, 0));
				assert_eq!(account_infos.0[2], (token_deposit_channel, 10000));

				let mut new_addresses = addresses.clone();
				new_addresses.push((empty_account, fetch_account_3));

				let account_infos = ingress_amounts(
					&retry_client,
					new_addresses,
					owner_account,
					true,
					Some(token_mint_pubkey),
				)
				.await;
				assert!(account_infos.is_err());

				let correct_data_account =
					SolAddress::from_str("HsRFLNzidLJx4RuqxdT924btgCTTuFFmNqp7Ph9y9HdN").unwrap();
				addresses.push((empty_account, correct_data_account));

				let account_infos = ingress_amounts(
					&retry_client,
					addresses,
					SolAddress::from_str("EVo1QjbAPKs4UbS78uNk2LpNG7hAWJum52ybQMtxgVL2").unwrap(),
					true,
					Some(token_mint_pubkey),
				)
				.await
				.unwrap();
				assert_eq!(account_infos.0[3], (empty_account, 74510874563096));

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
