use crate::witness::common::{RuntimeCallHasChain, RuntimeHasChain};
use anyhow::ensure;
use cf_chains::{
	instances::ChainInstanceFor,
	sol::{SolAddress, SolAsset, SolHash},
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

// TODO: This code supports not having to filter the asset in the deposit channels so we could
// call this together for native and tokens. The advantatge is that we can then use the same
// getMultipleAccounts for any asset. However, there is two issues:
// 1. If it's not native then we are using `token_pubkey`. If at some point we are to support more
//    tokens we should have a way to know which mint_pub_key to use for each token. This is a minor
//    problem
// 2. The main issue is that with the current pallet we have to submit a vector for all deposit
//    channels of the same asset. Therefore we need to split them up before submitting the
//    extrinsic. If the new pallet also works like that then we could also split the logic here (or
//    make token_pubkey an Option) since we will call them separately per asset anyway. However, if
//    the new pallet takes in separate extrinsics for each deposit channel then having the same
//    logic here is better.
//
// For now we have the underlying logic that supports both but we filter it first just because the
// way we are submitting the extrinsic.
impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	/// We track Solana (Sol and SPL-token) deposits by periodically querying the
	/// state of the deposit channel accounts. To ensure that no deposits are missed
	/// upon fetch, we use a helper program for each deposit channel (FetchHistoricalAccount)
	/// that keeps track of the cumulative amount fetched from the corresponding deposit
	/// channel.
	/// Using the deposit channel's balance and the cumulative amount fetched from it we
	/// can reliably track the amount deposited to a deposit channel.
	/// As a reminder, for token deposit channels the account to witness is the derived
	/// associated account from the actual deposit channel provided by the State Chain.
	pub async fn sol_deposits<ProcessCall, ProcessingFut, SolRetryRpcClient>(
		self,
		process_call: ProcessCall,
		sol_rpc: SolRetryRpcClient,
		asset: <Inner::Chain as cf_chains::Chain>::ChainAsset,
		vault_address: SolAddress,
		cached_balances: Arc<Mutex<HashMap<SolAddress, u128>>>,
		token_pubkey: Option<SolAddress>,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner::Chain: cf_chains::Chain<
			ChainAmount = u64,
			DepositDetails = (),
			ChainAccount = SolAddress,
			ChainAsset = SolAsset,
		>,
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

				println!("DEBUGDEPOSITS Processing Solana Deposits Inner 2");

				// Genesis block cannot contain any transactions
				if !deposit_channels.is_empty() {
					let deposit_channels_info = deposit_channels
						.into_iter()
						// We filter them for now to facilitate the submission of the extrinsic.
						.filter(|deposit_channel| deposit_channel.deposit_channel.asset == asset)
						.map(|deposit_channel| {
							match deposit_channel.deposit_channel.asset {
								// Native deposit channel
								cf_primitives::chains::assets::sol::Asset::Sol =>
									DepositChannelType::NativeDepositChannel(
										deposit_channel.deposit_channel.address,
									),
								_ => {
									let token_pubkey =
										token_pubkey.expect("Token pubkey not provided");
									DepositChannelType::TokenDepositChannel(
										deposit_channel.deposit_channel.address,
										derive_associated_token_account(
											deposit_channel.deposit_channel.address,
											token_pubkey,
										)
										.expect("Failed to derive associated token account")
										.0,
										token_pubkey,
									)
								},
							}
						})
						.collect::<Vec<_>>();

					println!(
						"DEBUGDEPOSITS Processing Solana Deposits {:?}",
						deposit_channels_info
					);

					let chunked_deposit_channels_info = deposit_channels_info
						.chunks(MAX_MULTIPLE_ACCOUNTS_QUERY / 2)
						.map(|chunk| chunk.to_vec())
						.collect::<Vec<Vec<_>>>();

					// TODO: Should submit every deposit channel separately with the new pallet?
					for deposit_channels_info in chunked_deposit_channels_info {
						let (ingresses, slot) =
							ingress_amounts(&sol_rpc, deposit_channels_info, vault_address).await?;

						let ingresses: Vec<(SolAddress, u128)> =
							ingresses.into_iter().filter(|&(_, amount)| amount != 0).collect();

						if !ingresses.is_empty() {
							let mut cached_balances = cached_balances.lock().await;

							// Filter out the deposit channels that have the same balance
							let new_ingresses: Vec<(SolAddress, u128)> = ingresses
								.into_iter()
								.filter_map(|(deposit_channel_address, amount)| {
									let deposit_channel_cached_balance =
										cached_balances.get(&deposit_channel_address).unwrap_or(&0);

									println!(
										"DEBUGDEPOSITS deposit_channel_address {:?}, cached_balance {:?}, amount {:?}, ",
										deposit_channel_address,
										*deposit_channel_cached_balance,
										amount
									);

									if amount > *deposit_channel_cached_balance {
										// With the current pallet we submit the difference in
										// amount. This is a temporal workaround TODO: Should submit
										// the amount as is with the new pallet?
										// Some((deposit_channel_address, amount))
										Some((
											deposit_channel_address,
											amount - deposit_channel_cached_balance,
										))
									} else {
										None
									}
								})
								.collect::<Vec<_>>();

							println!("DEBUGDEPOSITS Submitting new_ingresses {:?}", new_ingresses);

							process_call(
								pallet_cf_ingress_egress::Call::<_, ChainInstanceFor<Inner::Chain>>::process_deposits {
									deposit_witnesses: new_ingresses.clone()
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
									block_height: slot,
								}
								.into(),
								epoch.index,
							)
							.await;

							// Update hashmap
							new_ingresses.into_iter().for_each(
								|(deposit_channel_address, value)| {
									println!(
									"DEBUGDEPOSITS Updating cached_balances for {:?} to value {:?}",
									deposit_channel_address, value
								);
									cached_balances.insert(deposit_channel_address, value);
								},
							);
						}
					}
				}
				Ok::<_, anyhow::Error>(())
			}
		})
	}
}

// Returns the ingress amounts per deposit channel from a vector of deposit channels that contain
// the deposit channel address type. Token deposit channel must have been derived previously.
async fn ingress_amounts<SolRetryRpcClient>(
	sol_rpc: &SolRetryRpcClient,
	deposit_channels: Vec<DepositChannelType>,
	vault_address: SolAddress,
) -> Result<(Vec<(SolAddress, u128)>, u64), anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let addresses_to_witness = deposit_channels
		.clone()
		.into_iter()
		.flat_map(|deposit_channel| {
			vec![
				*deposit_channel.address_to_witness(),
				derive_fetch_account(vault_address, *deposit_channel.deposit_channel_address())
					.expect("Failed to derive fetch account"),
			]
		})
		.collect::<Vec<_>>();

	ensure!(addresses_to_witness.len() <= MAX_MULTIPLE_ACCOUNTS_QUERY);
	ensure!(addresses_to_witness.len() > 0);

	let accounts_info_response: Response<Vec<Option<UiAccount>>> = sol_rpc
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

	let slot = accounts_info_response.context.slot;
	let accounts_info = accounts_info_response.value;

	let mut ingresses = Vec::new();

	for (deposit_channel, chunked_accounts_info) in
		deposit_channels.iter().zip(accounts_info.chunks(2))
	{
		let accumulated_amount = match chunked_accounts_info {
			[deposit_channel_account_info, fetch_account_info] => {
				let deposit_channel_balance = parse_account_amount_from_data(
					deposit_channel_account_info.clone(),
					AccountType::DepositChannelType(deposit_channel.clone()),
					vault_address,
				)
				.expect("Failed to get deposit channel balance");
				let fetch_account_balance = parse_account_amount_from_data(
					fetch_account_info.clone(),
					AccountType::FetchAccount,
					vault_address,
				)
				.expect("Failed to get fetch account balance");

				// We add up the values and push them if they are greater than 0
				Ok(deposit_channel_balance.saturating_add(fetch_account_balance))
			},
			// We shouldn't get zero accounts nor an odd number
			_ => Err(anyhow::anyhow!("Unexpected number of accounts returned")),
		}?;

		ingresses.push((*deposit_channel.deposit_channel_address(), accumulated_amount));
	}
	Ok((ingresses, slot))
}

fn parse_account_amount_from_data(
	account_info: Option<UiAccount>,
	account_params: AccountType,
	vault_address: SolAddress,
) -> Result<u128, anyhow::Error> {
	let system_program_pubkey = SolAddress::from_str(SYSTEM_PROGRAM_ID).unwrap();
	let associated_token_account_pubkey = SolAddress::from_str(TOKEN_PROGRAM_ID).unwrap();
	match account_info {
		Some(account_info) => {
			println!("Parsing account_info {:?}", account_info);

			let owner_pub_key = SolAddress::from_str(account_info.owner.as_str()).unwrap();
			println!("owner_pub_key {:?}", owner_pub_key);

			match account_params {
				AccountType::DepositChannelType(deposit_channel_type) => match deposit_channel_type
				{
					DepositChannelType::NativeDepositChannel(_sol_address) => {
						// Native deposit channel
						println!("Native deposit channel found");
						ensure!(
							owner_pub_key == system_program_pubkey,
							"Unexpected owner for native deposit channel",
						);
						Ok(account_info.lamports as u128)
					},
					DepositChannelType::TokenDepositChannel(
						deposit_channel_address,
						_,
						token_mint_pubkey,
					) => {
						// Token deposit channel
						println!("Token deposit channel found");
						ensure!(
							owner_pub_key == associated_token_account_pubkey,
							"Unexpected owner for token deposit channel"
						);

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

								ensure!(owner == deposit_channel_address.to_string());

								let mint = info
									.get("mint")
									.and_then(|v| v.as_str())
									.ok_or(anyhow::anyhow!("Missing 'mint' field"))?;

								ensure!(mint == token_mint_pubkey.to_string());

								info.get("tokenAmount")
									.and_then(|token_amount| token_amount.get("amount"))
									.and_then(|v| v.as_str())
									.ok_or(anyhow::anyhow!(
										"Missing 'tokenAmount' or 'amount' field"
									))?
									.parse()
									.map_err(|_| anyhow::anyhow!("Failed to parse string to u128"))
							},
							_ => Err(anyhow::anyhow!("Data account encoding is not JsonParsed")),
						}
					},
				},
				AccountType::FetchAccount => {
					// Fetch account
					println!("Fetch account found");
					ensure!(owner_pub_key == vault_address, "Unexpected owner for fetch account");
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

							Ok(u128::from_le_bytes(array))
						},
						_ => Err(anyhow::anyhow!("Data account encoding is not base64")),
					}
				},
			}
		},
		None => {
			println!("Empty account found");
			Ok(0_u128)
		},
	}
}

#[derive(Debug, Clone)]
enum AccountType {
	DepositChannelType(DepositChannelType),
	FetchAccount,
}

#[derive(Debug, Clone)]
enum DepositChannelType {
	// Deposit channel address
	NativeDepositChannel(SolAddress),
	// Deposit channel address, Deposit Channel ATA, mintPubkey
	TokenDepositChannel(SolAddress, SolAddress, SolAddress),
}
impl DepositChannelType {
	fn deposit_channel_address(&self) -> &SolAddress {
		match self {
			DepositChannelType::NativeDepositChannel(deposit_channel_address) =>
				deposit_channel_address,
			DepositChannelType::TokenDepositChannel(deposit_channel_address, _, _) =>
				deposit_channel_address,
		}
	}
	fn address_to_witness(&self) -> &SolAddress {
		match self {
			DepositChannelType::NativeDepositChannel(native_deposit_channel) =>
				native_deposit_channel,
			DepositChannelType::TokenDepositChannel(_, deposit_channel_ata, _) =>
				deposit_channel_ata,
		}
	}
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

	// 	#[test]
	// 	fn test_sol_ingresses_error_if_odd_length() {
	// 		let account_infos = vec![
	// 			(SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ").unwrap(), 1),
	// 			(SolAddress::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz").unwrap(), 2),
	// 			(SolAddress::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt").unwrap(), 3),
	// 		];

	// 		let ingresses = sol_ingresses(account_infos);

	// 		assert!(ingresses.is_err());
	// 	}

	// 	#[test]
	// 	fn test_sol_derive_fetch_account() {
	// 		let fetch_account = derive_fetch_account(
	// 			SolAddress::from_str("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf").unwrap(),
	// 			SolAddress::from_str("HAMxiXdEJxiBHabZAUm8PSLvWQM2GHi5PArVZvUCeDab").unwrap(),
	// 		)
	// 		.unwrap();
	// 		assert_eq!(
	// 			fetch_account,
	// 			SolAddress::from_str("HGgUaHpsmZpB3pcYt8PE89imca6BQBRqYtbVQQqsso3o").unwrap()
	// 		);
	// 	}

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

				let vault_program_account =
					SolAddress::from_str("gSbePebfvPy7tRqimPoVecS2UsBvYv46ynrzWocc92s").unwrap();

				let account_infos = vec![
					DepositChannelType::NativeDepositChannel(
						SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ")
							.unwrap(),
					),
					DepositChannelType::TokenDepositChannel(
						SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ")
							.unwrap(),
						SolAddress::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz")
							.unwrap(),
						SolAddress::from_str("MAZEnmTmMsrjcoD6vymnSoZjzGF7i7Lvr2EXjffCiUo")
							.unwrap(),
					),
				];

				let ingresses =
					ingress_amounts(&retry_client, account_infos.clone(), vault_program_account)
						.await
						.unwrap();

				assert_eq!(ingresses.0.len(), 2);

				// Check deposit addresses
				assert_eq!(ingresses.0[0].0, *account_infos[0].deposit_channel_address());
				assert_eq!(ingresses.0[1].0, *account_infos[1].deposit_channel_address());

				// Amounts
				assert!(ingresses.0[0].1 > 1990030941101);
				assert_eq!(ingresses.0[1].1, 10000);

				// Check slot
				assert!(ingresses.1 > 0);

				// Trying empty accounts
				let account_infos = vec![
					DepositChannelType::NativeDepositChannel(
						SolAddress::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt")
							.unwrap(),
					),
					DepositChannelType::TokenDepositChannel(
						SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ")
							.unwrap(),
						SolAddress::from_str("ADtaKHTsYSmsty3MgLVfTQoMpM7hvFNTd2AxwN3hWRtt")
							.unwrap(),
						SolAddress::from_str("MAZEnmTmMsrjcoD6vymnSoZjzGF7i7Lvr2EXjffCiUo")
							.unwrap(),
					),
				];
				let ingresses =
					ingress_amounts(&retry_client, account_infos.clone(), vault_program_account)
						.await
						.unwrap();

				assert_eq!(ingresses.0[0].1, 0);
				assert_eq!(ingresses.0[1].1, 0);

				// TODO: Deploy a new fetch account so we can test or refactor so we can test Fetch
				// accounts by itself

				// 				let mut new_addresses = addresses.clone();
				// 				new_addresses.push((empty_account, fetch_account_3));
				// 				let account_infos = ingress_amounts(
				// 					&retry_client,
				// 					new_addresses,
				// 					vault_program_account_account,
				// 					Some(token_mint_pubkey),
				// 				)
				// 				.await;
				// 				assert!(account_infos.is_err());

				// 				// Try real fetch data account
				// 				let real_fetch_data_account =
				// 					SolAddress::from_str("HsRFLNzidLJx4RuqxdT924btgCTTuFFmNqp7Ph9y9HdN").unwrap();
				// 				addresses.push((empty_account, real_fetch_data_account));

				// 				let account_infos = ingress_amounts(
				// 					&retry_client,
				// 					addresses.clone(),
				// 					SolAddress::from_str("EVo1QjbAPKs4UbS78uNk2LpNG7hAWJum52ybQMtxgVL2").unwrap(),
				// 					Some(token_mint_pubkey),
				// 				)
				// 				.await
				// 				.unwrap();
				// 				assert_eq!(account_infos.0[2], (empty_account, 74510874563096));

				// 				// Wrong vault address (owner) when checking ownership of a fetch account
				// 				assert!(ingress_amounts(
				// 					&retry_client,
				// 					addresses.clone(),
				// 					SolAddress::from_str("So11111111111111111111111111111111111111112").unwrap(),
				// 					Some(token_mint_pubkey),
				// 				)
				// 				.await
				// 				.is_err());

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
