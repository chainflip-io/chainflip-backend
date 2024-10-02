use std::collections::HashSet;

use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
	rpc_client_api::{RpcAccountInfoConfig, UiAccount, UiAccountData, UiAccountEncoding},
};
use anyhow::ensure;
use anyhow::{anyhow /* ensure */};
use base64::Engine;
use borsh::{BorshDeserialize, BorshSerialize};
use cf_chains::sol::SolAddress;
use itertools::Itertools;

const SWAP_ENDPOINT_DATA_ACCOUNT_DISCRIMINATOR: [u8; 8] = [79, 152, 191, 225, 128, 108, 11, 139];
const SWAP_EVENT_ACCOUNT_DISCRIMINATOR: [u8; 8] = [150, 251, 114, 94, 200, 113, 248, 70];

#[derive(BorshDeserialize, BorshSerialize, Debug)]
struct SwapEndpointDataAccount {
	discriminator: [u8; 8],
	historical_number_event_accounts: u128,
	open_event_accounts: Vec<[u8; sol_prim::consts::SOLANA_ADDRESS_LEN]>,
}

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct SwapEvent {
	pub sender: [u8; sol_prim::consts::SOLANA_ADDRESS_LEN],
	pub dst_chain: u32,
	pub dst_address: Vec<u8>,
	pub dst_token: u32,
	pub amount: u64,
	pub src_token: Option<[u8; sol_prim::consts::SOLANA_ADDRESS_LEN]>,
	pub ccm_parameters: Option<CcmParams>,
	pub cf_parameters: Vec<u8>,
}

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct CcmParams {
	pub message: Vec<u8>,
	pub gas_amount: u64,
}

// 1. Query opened accounts on chain
// 2. Check the returned accounts against the SC opened_accounts
// 3. If they are already seen in the SC we do nothing with them
// 4. If they are not seen in the SC we query the account data. Then we parse the account data and
//    ensure it's a valid a program swap. The new program swap needs to be reported to the SC.
// 5. If an account is in the SC but not see in the engine we report it as closed.

// NOTE: Be aware that it can be that we query the open accounts, we get a list and then one
// of the accounts get closed while we query for a particular account. This is not a problem
// but we should handle correctly.

pub async fn get_program_swaps(
	sol_rpc: &SolRetryRpcClient,
	sc_opened_accounts: Vec<SolAddress>,
	swap_endpoint_data_account_address: SolAddress,
) -> Result<(Vec<SolAddress>, Vec<SolAddress>), anyhow::Error> {
	let (_historical_number_event_accounts, open_event_accounts) =
		get_event_accounts(sol_rpc, swap_endpoint_data_account_address)
			.await
			.expect("Failed to get the event accounts");

	// NOTE: With the current architecture where the SC is the source of truth for the opened
	// channels we can rely on that to not query the same accounts multiple times. The pallet should
	// also filter votes when voting multiple times while consensus is not reached. Therefore,
	// we shouldn't need the historical_number_event_accounts for now.
	let sc_opened_accounts_hashset: HashSet<_> = sc_opened_accounts.iter().collect();
	let mut new_program_swap_accounts = Vec::new();
	let mut closed_accounts = Vec::new();

	for account in &open_event_accounts {
		if !sc_opened_accounts_hashset.contains(account) {
			new_program_swap_accounts.push(*account);
		}
	}

	let open_event_accounts_hashset: HashSet<_> = open_event_accounts.iter().collect();
	for account in sc_opened_accounts {
		if !open_event_accounts_hashset.contains(&account) {
			closed_accounts.push(account);
		}
	}

	Ok((new_program_swap_accounts, closed_accounts))
}

pub async fn get_event_accounts(
	sol_rpc: &SolRetryRpcClient,
	swap_endpoint_data_account_address: SolAddress,
) -> Result<(u128, Vec<SolAddress>), anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let accounts_info = sol_rpc
		.get_multiple_accounts(
			&[swap_endpoint_data_account_address],
			RpcAccountInfoConfig {
				encoding: Some(UiAccountEncoding::Base64),
				data_slice: None,
				commitment: Some(CommitmentConfig::finalized()),
				min_context_slot: None,
			},
		)
		.await
		.value
		.into_iter()
		.exactly_one()
		.expect("We queried for exactly one account.");

	match accounts_info {
		Some(UiAccount { data: UiAccountData::Binary(base64_string, encoding), .. }) => {
			if encoding != UiAccountEncoding::Base64 {
				return Err(anyhow!("Data account encoding is not base64"));
			}
			let bytes = base64::engine::general_purpose::STANDARD
				.decode(base64_string)
				.expect("Failed to decode base64 string");

			// 8 Discriminator + 16 Historical Number Event Accounts + 4 bytes length + optional
			if bytes.len() < 28 {
				return Err(anyhow!("Expected account to have at least 28 bytes"));
			}

			let deserialized_data: SwapEndpointDataAccount =
				SwapEndpointDataAccount::try_from_slice(&bytes)
					.map_err(|e| anyhow!("Failed to deserialize data: {:?}", e))?;

			ensure!(
				deserialized_data.discriminator == SWAP_ENDPOINT_DATA_ACCOUNT_DISCRIMINATOR,
				"Discriminator does not match expected value"
			);

			Ok((
				deserialized_data.historical_number_event_accounts,
				deserialized_data.open_event_accounts.into_iter().map(SolAddress).collect(),
			))
		},
		Some(_) =>
			Err(anyhow!("Expected UiAccountData::Binary(String, UiAccountEncoding::Base64)")),
		None => Err(anyhow!(
			"Expected swap_endpoint_data_account_address to be found: {:?}",
			swap_endpoint_data_account_address
		)),
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{HttpEndpoint, NodeContainer},
		sol::retry_rpc::SolRetryRpcClient,
	};

	use cf_chains::{Chain, Solana};
	use futures_util::FutureExt;
	use std::str::FromStr;
	use utilities::task_scope;

	use super::*;

	#[tokio::test]
	// #[ignore]
	async fn test_get_event_accounts() {
		task_scope::task_scope(|scope| {
			async {
				let client = SolRetryRpcClient::new(
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

				let (historical_number_event_accounts, open_event_accounts) = get_event_accounts(
					&client,
					// Swap Endpoint Data Account Address with no opened accounts
					SolAddress::from_str("BckDu65u2ofAfaSDDEPg2qJTufKB4PvGxwcYhJ2wkBTC").unwrap(),
				)
				.await
				.unwrap();

				assert_eq!(historical_number_event_accounts, 0_u128);
				assert_eq!(open_event_accounts.len(), 0);

				let (historical_number_event_accounts, open_event_accounts) = get_event_accounts(
					&client,
					// Swap Endpoint Data Account Address with two opened accounts
					SolAddress::from_str("72HKrbbesW9FGuBoebns77uvY9fF9MEsw4HTMEeV53W9").unwrap(),
				)
				.await
				.unwrap();

				assert_eq!(historical_number_event_accounts, 2_u128);
				assert_eq!(
					open_event_accounts,
					vec![
						SolAddress::from_str("HhxGAt8THMtsW97Zuo5ZrhKgqsdD5EBgCx9vZ4n62xpf")
							.unwrap(),
						SolAddress::from_str("E81G7Q1BjierakQCfL9B5Tm485eiaRPb22bcKD2vtRfU")
							.unwrap()
					]
				);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	// #[ignore]
	async fn test_get_program_swaps() {
		task_scope::task_scope(|scope| {
			async {
				let client = SolRetryRpcClient::new(
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

				let (new_program_swap_accounts, closed_accounts) = get_program_swaps(
					&client,
					vec![SolAddress::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
						.unwrap()],
					// Swap Endpoint Data Account Address with no opened accounts
					SolAddress::from_str("BckDu65u2ofAfaSDDEPg2qJTufKB4PvGxwcYhJ2wkBTC").unwrap(),
				)
				.await
				.unwrap();

				assert_eq!(new_program_swap_accounts, vec![]);
				assert_eq!(
					closed_accounts,
					vec![SolAddress::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
						.unwrap()]
				);

				let (new_program_swap_accounts, closed_accounts) = get_program_swaps(
					&client,
					vec![SolAddress::from_str("HhxGAt8THMtsW97Zuo5ZrhKgqsdD5EBgCx9vZ4n62xpf")
						.unwrap()],
					// Swap Endpoint Data Account Address with two opened accounts
					SolAddress::from_str("72HKrbbesW9FGuBoebns77uvY9fF9MEsw4HTMEeV53W9").unwrap(),
				)
				.await
				.unwrap();

				println!("new_program_swap_accounts: {:?}", new_program_swap_accounts);
				println!("closed_accounts: {:?}", closed_accounts);

				assert_eq!(
					new_program_swap_accounts,
					vec![SolAddress::from_str("E81G7Q1BjierakQCfL9B5Tm485eiaRPb22bcKD2vtRfU")
						.unwrap()]
				);
				assert_eq!(closed_accounts, vec![]);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
