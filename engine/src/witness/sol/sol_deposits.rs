use anyhow::ensure;
use base64::Engine;
use cf_chains::sol::{
	sol_tx_core::address_derivation::{derive_associated_token_account, derive_fetch_account},
	SolAddress, SolAmount,
};
use cf_primitives::chains::assets::sol::Asset;
use futures::{stream, StreamExt, TryStreamExt};
use pallet_cf_elections::electoral_systems::blockchain::delta_based_ingress::{
	ChannelTotalIngressed, ChannelTotalIngressedFor, OpenChannelDetailsFor,
};
use serde_json::Value;
use sp_runtime::SaturatedConversion;
use state_chain_runtime::SolanaIngressEgress;
use std::{collections::BTreeMap, str::FromStr};

use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::SolRetryRpcApi,
	rpc_client_api::{
		ParsedAccount, Response, RpcAccountInfoConfig, UiAccount, UiAccountData, UiAccountEncoding,
	},
};

pub use sol_prim::consts::{SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID};

// 16 (u128) + 8 (discriminator)
const FETCH_ACCOUNT_BYTE_LENGTH: usize = 24;
const MAX_MULTIPLE_ACCOUNTS_QUERY: usize = 100;
const FETCH_ACCOUNT_DISCRIMINATOR: [u8; 8] = [188, 68, 197, 38, 48, 192, 81, 100];
const MAXIMUM_CONCURRENT_RPCS: usize = 16;

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
pub async fn get_channel_ingress_amounts<SolRetryRpcClient>(
	sol_rpc: &SolRetryRpcClient,
	vault_address: SolAddress,
	token_pubkey: SolAddress,
	deposit_channels: BTreeMap<
		SolAddress,
		(OpenChannelDetailsFor<SolanaIngressEgress>, ChannelTotalIngressedFor<SolanaIngressEgress>),
	>,
) -> Result<BTreeMap<SolAddress, ChannelTotalIngressedFor<SolanaIngressEgress>>, anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let deposit_channels = &deposit_channels;
	stream::iter(
		deposit_channels
			.clone() // https://github.com/rust-lang/rust/issues/110338 && https://github.com/rust-lang/rust/issues/126551
			.into_iter()
			.map(|(address, (open_channel_details, _current_total_ingressed))| {
				match open_channel_details.asset {
					// Native deposit channel
					Asset::Sol => DepositChannelType::NativeDepositChannel(address),
					// Token deposit channel. If we ever want to add another
					// token we should add a match arm here and adequately
					// pass a corresponding token_pubkey to this function.
					Asset::SolUsdc => DepositChannelType::TokenDepositChannel(
						address,
						derive_associated_token_account(address, token_pubkey)
							.expect("Failed to derive associated token account")
							.address,
						token_pubkey,
					),
				}
			}),
	)
	.chunks(MAX_MULTIPLE_ACCOUNTS_QUERY / 2)
	.map(|deposit_channels_chunk| ingress_amounts(sol_rpc, deposit_channels_chunk, vault_address))
	.buffered(MAXIMUM_CONCURRENT_RPCS)
	.map_ok(|(ingress_amounts_chunk, slot)| {
		stream::iter(
			ingress_amounts_chunk
				.into_iter()
				.filter_map(move |(deposit_channel, amount)| {
					let amount: SolAmount = amount.saturated_into(); // TODO: Change the DeltaBasedIngress to not use the Chains Amount type but
													  // instead use u256 or u128 or some generic.
					let (deposit_details, current_total_ingressed) =
						deposit_channels.get(deposit_channel.address()).unwrap();
					if amount != current_total_ingressed.amount ||
						slot < current_total_ingressed.block_number ||
						// We avoid submitting an extrinsic if the amount is unchanged. However, the delta_based_ingress election won't close a
						// channel until at least one ingress submission reaches consensus. To ensure this happens, we submit the amount on the
						// first witnessed slot after the deposit channel's close block.
						slot >= deposit_details.close_block
					{
						Some((
							*deposit_channel.address(),
							ChannelTotalIngressed { block_number: slot, amount },
						))
					} else {
						None
					}
				})
				.map(Ok),
		)
	})
	.try_flatten()
	.try_collect()
	.await
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
				derive_fetch_account(*address_to_witness, vault_address)
					.expect("Failed to derive fetch account")
					.address,
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

	ensure!(ingresses.len() == deposit_channels.len());

	Ok((ingresses, slot))
}

fn parse_account_amount_from_data(
	deposit_channel_info: UiAccount,
	deposit_channel: DepositChannelType,
) -> Result<u128, anyhow::Error> {
	let owner_pub_key = SolAddress::from_str(deposit_channel_info.owner.as_str()).unwrap();

	match deposit_channel {
		DepositChannelType::NativeDepositChannel(_) => {
			ensure!(
				owner_pub_key == SYSTEM_PROGRAM_ID,
				"Unexpected owner for native deposit channel",
			);
			Ok(deposit_channel_info.lamports as u128)
		},
		DepositChannelType::TokenDepositChannel(deposit_channel_address, _, token_mint_pubkey) => {
			ensure!(
				owner_pub_key == TOKEN_PROGRAM_ID,
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
				_ => Err(anyhow::anyhow!(
					"Data account encoding is not JsonParsed for account {:?}: {:?}",
					deposit_channel.address_to_witness(),
					deposit_channel_info.data
				)),
			}
		},
	}
}

fn parse_fetch_account_amount(
	fetch_account_info: UiAccount,
	vault_address: SolAddress,
) -> Result<u128, anyhow::Error> {
	let owner_pub_key = SolAddress::from_str(fetch_account_info.owner.as_str()).unwrap();

	ensure!(owner_pub_key == vault_address, "Unexpected owner for fetch account");
	match fetch_account_info.data {
		// Fetch Data Account
		UiAccountData::Binary(base64_string, encoding) => {
			if encoding != UiAccountEncoding::Base64 {
				return Err(anyhow::anyhow!("Data account encoding is not base64"));
			}

			let mut bytes = base64::engine::general_purpose::STANDARD
				.decode(base64_string)
				.expect("Failed to decode base64 string");

			ensure!(bytes.len() == FETCH_ACCOUNT_BYTE_LENGTH);

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
	TokenDepositChannel(SolAddress, SolAddress, SolAddress),
}
impl DepositChannelType {
	fn address(&self) -> &SolAddress {
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
		settings::{HttpEndpoint, NodeContainer},
		sol::retry_rpc::SolRetryRpcClient,
	};

	use cf_chains::{sol::SolAddress, Chain, Solana};
	use cf_utilities::task_scope;
	use futures_util::FutureExt;
	use std::str::FromStr;

	use super::*;

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_get_deposit_channels_info() {
		task_scope::task_scope(|scope| {
			async {
				let retry_client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: HttpEndpoint {
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

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	#[should_panic]
	async fn test_fail_erroneus_fetch_account() {
		task_scope::task_scope(|scope| {
			async {
				let retry_client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: HttpEndpoint {
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
