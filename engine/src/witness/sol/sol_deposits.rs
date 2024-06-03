use crate::witness::common::{RuntimeCallHasChain, RuntimeHasChain};
use anyhow::ensure;
use cf_chains::{
	instances::ChainInstanceFor,
	sol::{SolAddress, SolAsset, SolHash},
	Chain,
};
use cf_primitives::{chains::assets::sol::Asset, EpochIndex};
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

// 16 (u128) + 8 (discriminator)
const FETCH_ACCOUNT_BYTE_LENGTH: usize = 24;
const MAX_MULTIPLE_ACCOUNTS_QUERY: usize = 100;
const FETCH_ACCOUNT_DISCRIMINATOR: [u8; 8] = [188, 68, 197, 38, 48, 192, 81, 100];

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
	/// Native asset and tokens are processed together reusing the getMultipleAccounts
	/// for any combination of deposit channels and assets.
	pub async fn sol_deposits<ProcessCall, ProcessingFut, SolRetryRpcClient>(
		self,
		process_call: ProcessCall,
		sol_rpc: SolRetryRpcClient,
		vault_address: SolAddress,
		cached_balances: Arc<Mutex<HashMap<SolAddress, u128>>>,
		token_pubkey: SolAddress,
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
		self.then(move |epoch, header| {
			assert!(<Inner::Chain as Chain>::is_block_witness_root(header.index));

			let sol_rpc = sol_rpc.clone();
			let process_call = process_call.clone();
			let cached_balances = Arc::clone(&cached_balances);

			async move {
				let (_, deposit_channels) = header.data;

				// Genesis block cannot contain any transactions
				if !deposit_channels.is_empty() {
					let deposit_channels_info = deposit_channels
						.into_iter()
						.map(|deposit_channel| {
							match deposit_channel.deposit_channel.asset {
								// Native deposit channel
								Asset::Sol => DepositChannelType::NativeDepositChannel(
									deposit_channel.deposit_channel.address,
								),
								// Token deposit channel. If we ever want to add another
								// token we should add a match arm here and adequately
								// pass a corresponding token_pubkey to this function.
								Asset::SolUsdc => DepositChannelType::TokenDepositChannel(
									deposit_channel.deposit_channel.address,
									derive_associated_token_account(
										deposit_channel.deposit_channel.address,
										token_pubkey,
									)
									.expect("Failed to derive associated token account")
									.0,
									token_pubkey,
									deposit_channel.deposit_channel.asset,
								),
							}
						})
						.collect::<Vec<_>>();

					println!(
						"DEBUGDEPOSITS Processing Solana Deposits {:?}",
						deposit_channels_info
					);

					if !deposit_channels_info.is_empty() {
						let chunked_deposit_channels_info = deposit_channels_info
							.chunks(MAX_MULTIPLE_ACCOUNTS_QUERY / 2)
							.map(|chunk| chunk.to_vec())
							.collect::<Vec<Vec<_>>>();

						for chunk_deposit_channels_info in chunked_deposit_channels_info {
							let (ingresses, slot) = ingress_amounts(
								&sol_rpc,
								chunk_deposit_channels_info,
								vault_address,
							)
							.await?;

							let ingresses: Vec<(DepositChannelType, u128)> =
								ingresses.into_iter().filter(|&(_, amount)| amount != 0).collect();

							if !ingresses.is_empty() {
								let mut cached_balances = cached_balances.lock().await;

								// Filter out the deposit channels that have the same balance
								let new_ingresses: Vec<(DepositChannelType, u128)> = ingresses
									.into_iter()
									.filter_map(|(deposit_channel, amount)| {
										let deposit_channel_cached_balance = cached_balances
											.get(deposit_channel.address_to_witness())
											.unwrap_or(&0);

										println!(
										"DEBUGDEPOSITS deposit_channel {:?}, cached_balance {:?}, amount {:?}, ",
										deposit_channel,
										*deposit_channel_cached_balance,
										amount
									);

										if amount > *deposit_channel_cached_balance {
											// With the current pallet we submit the difference in
											// amount. This is a temporal workaround TODO: Should
											// submit the amount as is with the new pallet?
											// Some((deposit_channel, amount))
											Some((
												deposit_channel,
												amount - deposit_channel_cached_balance,
											))
										} else {
											None
										}
									})
									.collect::<Vec<_>>();

								if !new_ingresses.is_empty() {
									println!(
										"DEBUGDEPOSITS Submitting new_ingresses {:?}",
										new_ingresses
									);

									process_call(
										pallet_cf_ingress_egress::Call::<
											_,
											ChainInstanceFor<Inner::Chain>,
										>::process_deposits {
											deposit_witnesses: new_ingresses
												.clone()
												.into_iter()
												.map(|(deposit_channel, amount)| {
													pallet_cf_ingress_egress::DepositWitness {
												deposit_address: *deposit_channel.address(),
												asset: deposit_channel.asset(),
												amount: amount
													.try_into()
													.expect("Ingress witness transfer value should fit u64"),
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
										|(deposit_channel, value)| {
											println!(
									"DEBUGDEPOSITS Updating cached_balances for {:?} to value {:?}",
									deposit_channel, value
								);
											cached_balances.insert(
												*deposit_channel.address_to_witness(),
												value,
											);
										},
									);
								}
							}
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
) -> Result<(Vec<(DepositChannelType, u128)>, u64), anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	ensure!(deposit_channels.len() <= MAX_MULTIPLE_ACCOUNTS_QUERY / 2);

	let addresses_to_witness = deposit_channels
		.clone()
		.into_iter()
		.flat_map(|deposit_channel| {
			let address_to_witness = deposit_channel.address_to_witness();
			vec![
				*address_to_witness,
				derive_fetch_account(vault_address, *address_to_witness)
					.expect("Failed to derive fetch account"),
			]
		})
		.collect::<Vec<_>>();

	ensure!(addresses_to_witness.len() <= MAX_MULTIPLE_ACCOUNTS_QUERY);

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
				let deposit_channel_balance = deposit_channel_account_info
					.as_ref()
					.map_or(Ok(0_u128), |deposit_channel_info| {
						parse_account_amount_from_data(
							deposit_channel_info.clone(),
							deposit_channel.clone(),
						)
					})
					.expect("Failed to get deposit channel balance");
				let fetch_account_balance = fetch_account_info
					.as_ref()
					.map_or(Ok(0_u128), |fetch_info| {
						parse_fetch_account_amount(fetch_info.clone(), vault_address)
					})
					.expect("Failed to get fetch account balance");

				// We add up the values and push them if they are greater than 0
				Ok(deposit_channel_balance.saturating_add(fetch_account_balance))
			},
			// We shouldn't get zero accounts nor an odd number
			_ => Err(anyhow::anyhow!("Unexpected number of accounts returned")),
		}?;

		ingresses.push((deposit_channel.clone(), accumulated_amount));
	}

	ensure!(deposit_channels.len() <= ingresses.len());

	Ok((ingresses, slot))
}

fn parse_account_amount_from_data(
	deposit_channel_info: UiAccount,
	deposit_channel: DepositChannelType,
) -> Result<u128, anyhow::Error> {
	println!("Parsing deposit_channel_info {:?}", deposit_channel_info);

	let owner_pub_key = SolAddress::from_str(deposit_channel_info.owner.as_str()).unwrap();

	match deposit_channel {
		DepositChannelType::NativeDepositChannel(_) => {
			// Native deposit channel
			println!("Native deposit channel found");
			let system_program_pubkey = SolAddress::from_str(SYSTEM_PROGRAM_ID).unwrap();
			ensure!(
				owner_pub_key == system_program_pubkey,
				"Unexpected owner for native deposit channel",
			);
			Ok(deposit_channel_info.lamports as u128)
		},
		DepositChannelType::TokenDepositChannel(
			deposit_channel_address,
			_,
			token_mint_pubkey,
			_,
		) => {
			// Token deposit channel
			println!("Token deposit channel found");

			let associated_token_account_pubkey = SolAddress::from_str(TOKEN_PROGRAM_ID).unwrap();
			ensure!(
				owner_pub_key == associated_token_account_pubkey,
				"Unexpected owner for token deposit channel"
			);

			// Fetch data and ensure it's encoding is JsonParsed
			match deposit_channel_info.data {
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
						.ok_or(anyhow::anyhow!("Missing 'tokenAmount' or 'amount' field"))?
						.parse()
						.map_err(|_| anyhow::anyhow!("Failed to parse string to u128"))
				},
				_ => Err(anyhow::anyhow!("Data account encoding is not JsonParsed")),
			}
		},
	}
}

fn parse_fetch_account_amount(
	fetch_account_info: UiAccount,
	vault_address: SolAddress,
) -> Result<u128, anyhow::Error> {
	println!("Parsing fetch_account_info {:?}", fetch_account_info);

	let owner_pub_key = SolAddress::from_str(fetch_account_info.owner.as_str()).unwrap();
	println!("owner_pub_key {:?}", owner_pub_key);

	ensure!(owner_pub_key == vault_address, "Unexpected owner for fetch account");
	match fetch_account_info.data {
		// Fetch Data Account
		UiAccountData::Binary(base64_string, encoding) => {
			if encoding != UiAccountEncoding::Base64 {
				return Err(anyhow::anyhow!("Data account encoding is not base64"));
			}

			let mut bytes = base64::decode(base64_string).expect("Failed to decode base64 string");

			ensure!(bytes.len() == FETCH_ACCOUNT_BYTE_LENGTH);

			// Remove the discriminator
			let discriminator: Vec<u8> = bytes.drain(..8).collect();

			ensure!(
				discriminator == FETCH_ACCOUNT_DISCRIMINATOR,
				"Discriminator does not match expected value"
			);

			let array: [u8; 16] = bytes.try_into().expect("Byte slice length doesn't match u128");

			Ok(u128::from_le_bytes(array))
		},
		_ => Err(anyhow::anyhow!("Data account encoding is not base64")),
	}
}

#[derive(Debug, Clone)]
enum DepositChannelType {
	// Deposit channel address
	NativeDepositChannel(SolAddress),
	// Deposit channel address, Deposit Channel ATA, mintPubkey, asset
	TokenDepositChannel(SolAddress, SolAddress, SolAddress, Asset),
}
impl DepositChannelType {
	fn address(&self) -> &SolAddress {
		match self {
			DepositChannelType::NativeDepositChannel(deposit_channel_address) =>
				deposit_channel_address,
			DepositChannelType::TokenDepositChannel(deposit_channel_address, _, _, _) =>
				deposit_channel_address,
		}
	}
	fn address_to_witness(&self) -> &SolAddress {
		match self {
			DepositChannelType::NativeDepositChannel(native_deposit_channel) =>
				native_deposit_channel,
			DepositChannelType::TokenDepositChannel(_, deposit_channel_ata, _, _) =>
				deposit_channel_ata,
		}
	}
	fn asset(&self) -> Asset {
		match self {
			DepositChannelType::NativeDepositChannel(_) => Asset::Sol,
			DepositChannelType::TokenDepositChannel(_, _, _, asset) => *asset,
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{NodeContainer, WsHttpEndpoints},
		sol::retry_rpc::SolRetryRpcClient,
	};

	use cf_chains::{sol::SolAddress, Chain, Solana};
	use futures_util::FutureExt;
	use std::str::FromStr;
	use utilities::task_scope;

	use super::*;

	#[tokio::test]
	async fn test_get_deposit_channels_info() {
		task_scope::task_scope(|scope| {
			async {
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
						Asset::SolUsdc,
					),
				];

				let ingresses =
					ingress_amounts(&retry_client, account_infos.clone(), vault_program_account)
						.await
						.unwrap();

				assert_eq!(ingresses.0.len(), 2);

				// Check deposit addresses
				assert_eq!(ingresses.0[0].0.address(), account_infos[0].address());
				assert_eq!(
					ingresses.0[0].0.address_to_witness(),
					account_infos[0].address_to_witness()
				);
				assert_eq!(ingresses.0[1].0.address(), account_infos[1].address());
				assert_eq!(
					ingresses.0[1].0.address_to_witness(),
					account_infos[1].address_to_witness()
				);

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
						Asset::SolUsdc,
					),
				];
				let ingresses =
					ingress_amounts(&retry_client, account_infos.clone(), vault_program_account)
						.await
						.unwrap();

				assert_eq!(ingresses.0[0].1, 0);
				assert_eq!(ingresses.0[1].1, 0);

				let vault_program_account =
					SolAddress::from_str("EMxiTBPTkGVkkbCMncu7j17gHyySojii4KhHwM36Hgz2").unwrap();

				// Reacl fetch account deployed: 9MyUhDE1ZXr2Vs2TyMXccwnAYnUvrXvtvWxvuaDG6TbY
				let account_infos = vec![
					DepositChannelType::NativeDepositChannel(
						// Address used to derive the fetch account deployed
						SolAddress::from_str("12pWMFau4wPS1cnucRiDMhrKbBg876k5QduXkwVnXESa")
							.unwrap(),
					),
					DepositChannelType::TokenDepositChannel(
						// Arbitrary address, won't be used
						SolAddress::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ")
							.unwrap(),
						// Address used to derive the fetch account deployed
						SolAddress::from_str("12pWMFau4wPS1cnucRiDMhrKbBg876k5QduXkwVnXESa")
							.unwrap(),
						SolAddress::from_str("MAZEnmTmMsrjcoD6vymnSoZjzGF7i7Lvr2EXjffCiUo")
							.unwrap(),
						Asset::SolUsdc,
					),
				];

				let ingresses =
					ingress_amounts(&retry_client, account_infos.clone(), vault_program_account)
						.await
						.unwrap();

				assert_eq!(ingresses.0[0].1, 123456789);
				assert_eq!(ingresses.0[1].1, 123456789);
				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	#[should_panic]
	async fn test_fail_erroneus_fetch_account() {
		task_scope::task_scope(|scope| {
			async {
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
					SolAddress::from_str("EMxiTBPTkGVkkbCMncu7j17gHyySojii4KhHwM36Hgz2").unwrap();

				// Fetch account with incorrect data: "8VwrasdevLHvX4ytxa6yRqLkfdyh3GhEDXUChkS3bZRP"
				let account_infos = vec![DepositChannelType::NativeDepositChannel(
					// Address used to derive the fetch account deployed
					SolAddress::from_str("45BRYhjqH4kf8ZrwQWg2NNtFB5gmZdE5khKCoS5EtFV4").unwrap(),
				)];

				let _ =
					ingress_amounts(&retry_client, account_infos.clone(), vault_program_account)
						.await;
				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
