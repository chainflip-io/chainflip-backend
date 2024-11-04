use super::super::common::cf_parameters::*;
use codec::Decode;
use std::collections::{BTreeSet, HashSet};

use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
	rpc_client_api::{RpcAccountInfoConfig, UiAccount, UiAccountData, UiAccountEncoding},
};
use anyhow::ensure;
use anyhow::{anyhow /* ensure */};
use base64::Engine;
use borsh::BorshDeserialize;
use cf_chains::{
	address::EncodedAddress,
	assets::sol::Asset as SolAsset,
	sol::{
		api::VaultSwapAccountAndSender,
		sol_tx_core::program_instructions::{
			SwapEndpointDataAccount, SwapEvent, ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH,
			SWAP_ENDPOINT_DATA_ACCOUNT_DISCRIMINATOR, SWAP_EVENT_ACCOUNT_DISCRIMINATOR,
		},
		SolAddress,
	},
	CcmChannelMetadata, CcmDepositMetadata, ForeignChainAddress,
};
use futures::{stream, StreamExt, TryStreamExt};
use itertools::Itertools;
use state_chain_runtime::chainflip::solana_elections::SolanaVaultSwapDetails;
use tracing::warn;

const MAXIMUM_CONCURRENT_RPCS: usize = 16;
// Querying less than 100 (rpc call max) as those event accounts can be quite big.
// Max length ~ 1300 bytes per account. We set it to 10 as an arbitrary number to
// avoid large queries.
const MAX_MULTIPLE_EVENT_ACCOUNTS_QUERY: usize = 10;

// 1. Query the on-chain list of opened accounts from SwapEndpointDataAccount.
// 2. Check the returned accounts against the SC opened_accounts. The SC is the source of truth for
//    the opened channels we can rely on that to not query the same accounts multiple times.
// 3. If they are already seen in the SC we do nothing with them and skip the query.
// 4. If an account is in the SC but not see in the engine we report it as closed.
// 5. If they are not seen in the SC we query the account data. Then we parse the account data and
//    ensure it's a valid a program swap. The new program swap needs to be reported to the SC.

pub async fn get_program_swaps(
	sol_rpc: &SolRetryRpcClient,
	swap_endpoint_data_account_address: SolAddress,
	sc_open_accounts: Vec<SolAddress>,
	sc_closure_initiated_accounts: BTreeSet<VaultSwapAccountAndSender>,
	usdc_token_mint_pubkey: SolAddress,
) -> Result<
	(Vec<(VaultSwapAccountAndSender, SolanaVaultSwapDetails)>, Vec<VaultSwapAccountAndSender>),
	anyhow::Error,
> {
	let (new_program_swap_accounts, closed_accounts, slot) = get_changed_program_swap_accounts(
		sol_rpc,
		sc_open_accounts,
		sc_closure_initiated_accounts,
		swap_endpoint_data_account_address,
	)
	.await?;

	if new_program_swap_accounts.is_empty() {
		return Ok((vec![], closed_accounts));
	}

	let new_swaps = stream::iter(new_program_swap_accounts)
		.chunks(MAX_MULTIPLE_EVENT_ACCOUNTS_QUERY)
		.map(|new_program_swap_accounts_chunk| {
			get_program_swap_event_accounts_data(sol_rpc, new_program_swap_accounts_chunk, slot)
		})
		.buffered(MAXIMUM_CONCURRENT_RPCS)
		.map_ok(|program_swap_account_data_chunk| {
			stream::iter(program_swap_account_data_chunk.into_iter().filter_map(
				|(account, program_swap_account_data)| match program_swap_account_data {
					Some(data)
						if (data.src_token.is_none() ||
							data.src_token.is_some_and(|addr| addr == usdc_token_mint_pubkey.0)) => {

								let (deposit_metadata, vault_swap_parameters) = match data.ccm_parameters {
									None => {
										let CfParameters { ccm_additional_data: (), vault_swap_parameters } =
											CfParameters::decode(&mut &data.cf_parameters[..]).map_err(|e| warn!("error while decoding CfParameters for solana vault swap: {}. Omitting swap", e)).ok()?;
										(None, vault_swap_parameters)
									},
									Some(ccm_parameters) => {
										let CfParameters { ccm_additional_data, vault_swap_parameters } =
											CcmCfParameters::decode(&mut &data.cf_parameters[..]).map_err(|e| warn!("error while decoding CcmCfParameters for solana vault swap: {}. Omitting swap", e)).ok()?;

										let deposit_metadata = Some(CcmDepositMetadata {
											source_chain: cf_primitives::ForeignChain::Solana, // TODO: Pass chain id from above?
											source_address: Some(ForeignChainAddress::Sol(data.sender.into())),
											channel_metadata: CcmChannelMetadata {
												message: ccm_parameters.message
													.to_vec()
													.try_into()
													.map_err(|_| anyhow!("Failed to deposit CCM: `message` too long.")).ok()?,
												gas_budget: ccm_parameters.gas_amount.into(),
												ccm_additional_data,
											},
										});
										(deposit_metadata, vault_swap_parameters)
									}
								};

								Some(Ok((VaultSwapAccountAndSender {
									vault_swap_account: account,
									swap_sender: data.sender.into()
								}, SolanaVaultSwapDetails {
									from: if data.src_token.is_none() {SolAsset::Sol} else {SolAsset::SolUsdc},
									deposit_amount: data.amount,
									destination_address: EncodedAddress::from_chain_bytes(data.dst_chain.try_into().map_err(|e| warn!("error while parsing destination chain for solana vault swap:{}. Omitting swap", e)).ok()?, data.dst_address.to_vec()).map_err(|e| warn!("failed to decode the destination chain address for solana vault swap:{}. Omitting swap", e)).ok()?,
									to: data.dst_token.try_into().map_err(|e| warn!("error while decoding destination token for solana vault swap: {}. Omitting swap", e)).ok()?,
									deposit_metadata,
									// TODO: These two will potentially be a TransactionId type
									swap_account: account,
									creation_slot: data.creation_slot,
									broker_fees: vault_swap_parameters.broker_fees,
									refund_params: Some(vault_swap_parameters.refund_params),
									dca_params: vault_swap_parameters.dca_params,
									boost_fee: vault_swap_parameters.boost_fee,
								})))
							}

					// It could happen that some account is closed between the queries. This should
					// not happen because:
					// 1. Accounts in `new_program_swap_accounts` can only be accounts that have
					//    newly been opened and they won't be closed until consensus is reached.
					// 2. If due to rpc load management the get event accounts rpc is queried at a
					//    slot < get swap endpoint data rpc slot, the min_context_slot will prevent
					//    it from being executed before that.
					// This could only happen if an engine is behind and were to see the account
					// opened and closed between queries. That's not reallistic as it takes minutes
					// for an account to be closed and even if it were to happen it's not
					// problematic as we'd have reached consensus and the engine would just filter
					// it out.
					None => {
						warn!("Event account not found for solana event account");
						None
					},
					_ => {
						warn!("Unsupported input token for the witnessed solana vault swap, omitting the swap and the swap account.");
						None
					},
				},
			))
		})
		.try_flatten()
		.try_collect()
		.await;

	new_swaps.map(|swaps| (swaps, closed_accounts))

	// TODO: When submitting data we could technically submit the slot when the SwapEvent was
	// queried for the new opened accounts. However, it's just easier to submit the slot when the
	// SwapEndpointDataAccount was queried for both closed accounts and new opened accounts.
}

async fn get_changed_program_swap_accounts(
	sol_rpc: &SolRetryRpcClient,
	sc_opened_accounts: Vec<SolAddress>,
	sc_closure_initiated_accounts: BTreeSet<VaultSwapAccountAndSender>,
	swap_endpoint_data_account_address: SolAddress,
) -> Result<(Vec<SolAddress>, Vec<VaultSwapAccountAndSender>, u64), anyhow::Error> {
	let (_historical_number_event_accounts, open_event_accounts, slot) =
		get_swap_endpoint_data(sol_rpc, swap_endpoint_data_account_address)
			.await
			.expect("Failed to get the event accounts");

	let sc_opened_accounts_hashset: HashSet<_> = sc_opened_accounts.iter().collect();
	let sc_closure_initiated_accounts_hashset = sc_closure_initiated_accounts
		.iter()
		.map(|VaultSwapAccountAndSender { vault_swap_account, .. }| vault_swap_account)
		.collect::<HashSet<_>>();

	let mut new_program_swap_accounts = Vec::new();
	let mut closed_accounts = Vec::new();

	for account in &open_event_accounts {
		if !sc_opened_accounts_hashset.contains(account) &&
			!sc_closure_initiated_accounts_hashset.contains(account)
		{
			new_program_swap_accounts.push(*account);
		}
	}

	let open_event_accounts_hashset: HashSet<_> = open_event_accounts.iter().collect();
	for account in sc_closure_initiated_accounts {
		if !open_event_accounts_hashset.contains(&account.vault_swap_account) {
			closed_accounts.push(account);
		}
	}

	Ok((new_program_swap_accounts, closed_accounts, slot))
}

// Query the list of opened accounts from SwapEndpointDataAccount. The Swap Endpoint program ensures
// that the list is updated atomically whenever a swap event account is opened or closed.
async fn get_swap_endpoint_data(
	sol_rpc: &SolRetryRpcClient,
	swap_endpoint_data_account_address: SolAddress,
) -> Result<(u128, Vec<SolAddress>, u64), anyhow::Error> {
	let accounts_info_response = sol_rpc
		.get_multiple_accounts(
			&[swap_endpoint_data_account_address],
			RpcAccountInfoConfig {
				encoding: Some(UiAccountEncoding::Base64),
				data_slice: None,
				commitment: Some(CommitmentConfig::finalized()),
				min_context_slot: None,
			},
		)
		.await;

	let slot = accounts_info_response.context.slot;
	let accounts_info = accounts_info_response
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

			// 8 Discriminator + 16 Historical Number Event Accounts + 4 bytes vector length + data
			if bytes.len() < ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH + 20 {
				return Err(anyhow!("Expected account to have at least 28 bytes"));
			}

			let deserialized_data: SwapEndpointDataAccount =
				SwapEndpointDataAccount::try_from_slice(&bytes)
					.map_err(|e| anyhow!("Failed to deserialize data: {:?}", e))?;

			ensure!(
				deserialized_data.discriminator == SWAP_ENDPOINT_DATA_ACCOUNT_DISCRIMINATOR,
				"Discriminator does not match. Found: {:?}",
				deserialized_data.discriminator
			);

			Ok((
				deserialized_data.historical_number_event_accounts,
				deserialized_data.open_event_accounts.into_iter().map(SolAddress).collect(),
				slot,
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

async fn get_program_swap_event_accounts_data(
	sol_rpc: &SolRetryRpcClient,
	program_swap_event_accounts: Vec<SolAddress>,
	min_context_slot: u64,
) -> Result<Vec<(SolAddress, Option<SwapEvent>)>, anyhow::Error> {
	let accounts_info_response = sol_rpc
		.get_multiple_accounts(
			program_swap_event_accounts.as_slice(),
			RpcAccountInfoConfig {
				encoding: Some(UiAccountEncoding::Base64),
				data_slice: None,
				commitment: Some(CommitmentConfig::finalized()),
				min_context_slot: Some(min_context_slot),
			},
		)
		.await;

	let _slot = accounts_info_response.context.slot;
	let accounts_info = accounts_info_response.value;

	ensure!(accounts_info.len() == program_swap_event_accounts.len());

	program_swap_event_accounts
		.into_iter()
		.zip(accounts_info.into_iter())
		.map(|(account, accounts_info)| match accounts_info {
			Some(UiAccount { data: UiAccountData::Binary(base64_string, encoding), .. }) => {
				if encoding != UiAccountEncoding::Base64 {
					return Err(anyhow!("Data account encoding is not base64"));
				}
				let bytes = base64::engine::general_purpose::STANDARD
					.decode(base64_string)
					.expect("Failed to decode base64 string");

				if bytes.len() < ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH {
					return Err(anyhow!("Expected account to have at least 8 bytes"));
				}

				let deserialized_data: SwapEvent = SwapEvent::try_from_slice(&bytes)
					.map_err(|e| anyhow!("Failed to deserialize data: {:?}", e))?;

				ensure!(
					deserialized_data.discriminator == SWAP_EVENT_ACCOUNT_DISCRIMINATOR,
					"Discriminator does not match. Found: {:?}",
					deserialized_data.discriminator
				);

				Ok((account, Some(deserialized_data)))
			},
			Some(_) =>
				Err(anyhow!("Expected UiAccountData::Binary(String, UiAccountEncoding::Base64)")),
			None => Ok((account, None)),
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{HttpEndpoint, NodeContainer},
		sol::retry_rpc::SolRetryRpcClient,
	};

	use cf_chains::{Chain, Solana};
	use cf_utilities::task_scope;
	use futures_util::FutureExt;
	use std::str::FromStr;

	use super::*;

	#[tokio::test]
	#[ignore]
	async fn test_get_swap_endpoint_data() {
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

				let (historical_number_event_accounts, open_event_accounts, _) =
					get_swap_endpoint_data(
						&client,
						// Swap Endpoint Data Account Address with no opened accounts
						SolAddress::from_str("BckDu65u2ofAfaSDDEPg2qJTufKB4PvGxwcYhJ2wkBTC")
							.unwrap(),
					)
					.await
					.unwrap();

				assert_eq!(historical_number_event_accounts, 0_u128);
				assert_eq!(open_event_accounts.len(), 0);

				let (historical_number_event_accounts, open_event_accounts, _) =
					get_swap_endpoint_data(
						&client,
						// Swap Endpoint Data Account Address with two opened accounts
						SolAddress::from_str("72HKrbbesW9FGuBoebns77uvY9fF9MEsw4HTMEeV53W9")
							.unwrap(),
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
	#[ignore]
	async fn test_get_changed_program_swap_accounts() {
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

				let (new_program_swap_accounts, closed_accounts, _) =
					get_changed_program_swap_accounts(
						&client,
						vec![],
						BTreeSet::from([VaultSwapAccountAndSender {
							vault_swap_account: SolAddress::from_str(
								"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
							)
							.unwrap(),
							swap_sender: Default::default(),
						}]),
						// Swap Endpoint Data Account Address with no opened accounts
						SolAddress::from_str("BckDu65u2ofAfaSDDEPg2qJTufKB4PvGxwcYhJ2wkBTC")
							.unwrap(),
					)
					.await
					.unwrap();

				assert_eq!(new_program_swap_accounts, vec![]);
				assert_eq!(
					closed_accounts,
					vec![VaultSwapAccountAndSender {
						vault_swap_account: SolAddress::from_str(
							"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
						)
						.unwrap(),
						swap_sender: Default::default(),
					}]
				);

				let (new_program_swap_accounts, closed_accounts, _) =
					get_changed_program_swap_accounts(
						&client,
						vec![],
						BTreeSet::from([VaultSwapAccountAndSender {
							vault_swap_account: SolAddress::from_str(
								"HhxGAt8THMtsW97Zuo5ZrhKgqsdD5EBgCx9vZ4n62xpf",
							)
							.unwrap(),
							swap_sender: Default::default(),
						}]),
						// Swap Endpoint Data Account Address with two opened accounts
						SolAddress::from_str("72HKrbbesW9FGuBoebns77uvY9fF9MEsw4HTMEeV53W9")
							.unwrap(),
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

	#[tokio::test]
	#[ignore]
	async fn test_get_program_swap_event_accounts_data() {
		task_scope::task_scope(|scope| {
			async {
				let client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: HttpEndpoint { http_endpoint: "http://127.0.0.1:8899".into() },
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let program_swap_event_accounts_data = get_program_swap_event_accounts_data(
					&client,
					vec![
						SolAddress::from_str("GNrA2Ztxv1tJF3G4NVPEQtbRb9uT8rXcEY6ddPfzpnnT")
							.unwrap(),
						SolAddress::from_str("8yeBhX5BB4L9MfDddhwzktdmzMeNUEcvgZGPWLD3HDDY")
							.unwrap(),
						SolAddress::from_str("Dd1k91cWt84qJoQr3FT7EXQpSaMtZtwPwdho7RbMWtEV")
							.unwrap(),
					],
					123,
				)
				.await
				.unwrap();

				println!(
					"program_swap_event_accounts_data: {:?}",
					program_swap_event_accounts_data
				);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
