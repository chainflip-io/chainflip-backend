use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
	rpc_client_api::{RpcAccountInfoConfig, UiAccount, UiAccountData, UiAccountEncoding},
};
use anyhow::{anyhow, bail, ensure, Context};
use base64::Engine;
use cf_chains::{
	address::EncodedAddress,
	assets::sol::Asset as SolAsset,
	cf_parameters::{
		CfParameters, VaultSwapParameters, VersionedCcmCfParameters, VersionedCfParameters,
	},
	sol::{
		api::VaultSwapAccountAndSender,
		sol_tx_core::program_instructions::{
			swap_endpoints::types::{SwapEndpointDataAccount, SwapEvent},
			ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH,
		},
		SolAddress,
	},
	CcmChannelMetadata, CcmDepositMetadata, ForeignChainAddress,
};
use codec::Decode;
use futures::{stream, StreamExt, TryStreamExt};
use itertools::Itertools;
use state_chain_runtime::chainflip::solana_elections::SolanaVaultSwapDetails;
use std::collections::{BTreeSet, HashSet};
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
	sc_open_accounts: HashSet<SolAddress>,
	sc_closure_initiated_accounts: BTreeSet<VaultSwapAccountAndSender>,
	usdc_token_mint_pubkey: SolAddress,
) -> Result<
	(
		Vec<(VaultSwapAccountAndSender, Option<SolanaVaultSwapDetails>)>,
		Vec<VaultSwapAccountAndSender>,
	),
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
		.map_ok(stream::iter)
		.try_flatten()
		.filter_map(|response| {
			futures::future::ready(
				response
					.inspect_err(|e| {
						tracing::error!("Error querying for program swap account data: {e:?}");
					})
					.ok(),
			)
		})
		.map(
			|(
				vault_swap_account,
				SwapEvent {
					creation_slot,
					sender,
					dst_chain,
					dst_address,
					dst_token,
					amount,
					src_token,
					ccm_parameters,
					cf_parameters,
				},
			)| {
				{
					let vault_swap_details = move || {
						let from_asset =
							if let Some(token) = src_token {
								if token == usdc_token_mint_pubkey.into() {
									SolAsset::SolUsdc
								} else {
									bail!("Unsupported input token for the witnessed solana vault swap.");
								}
							} else {
								SolAsset::Sol
							};

						let (
							deposit_metadata,
							VaultSwapParameters {
								refund_params,
								dca_params,
								boost_fee,
								broker_fee,
								affiliate_fees,
							},
						) = match ccm_parameters {
							None => {
								let VersionedCfParameters::V0(CfParameters {
									ccm_additional_data: (),
									vault_swap_parameters,
								}) = VersionedCfParameters::decode(&mut &cf_parameters[..])
									.map_err(|e| {
										anyhow!("Error while decoding VersionedCfParameters for solana vault swap: {}.", e)
									})?;
								(None, vault_swap_parameters)
							},
							Some(ccm_parameters) => {
								let VersionedCcmCfParameters::V0(CfParameters {
									ccm_additional_data,
									vault_swap_parameters
									}) = VersionedCcmCfParameters::decode(&mut &cf_parameters[..]).map_err(|e| {
											anyhow!("Error while decoding VersionedCcmCfParameters for solana vault swap: {}.", e)
										},
									)?;

								(
									Some(CcmDepositMetadata {
										source_chain: cf_primitives::ForeignChain::Solana,
										source_address: Some(ForeignChainAddress::Sol(
											sender.into(),
										)),
										channel_metadata: CcmChannelMetadata {
											message: ccm_parameters
												.message
												.to_vec()
												.try_into()
												.map_err(|_| {
													anyhow!(
														"Failed to deposit CCM: `message` too long."
													)
												})?,
											gas_budget: ccm_parameters.gas_amount.into(),
											ccm_additional_data,
										},
									}),
									vault_swap_parameters,
								)
							},
						};
						Ok(SolanaVaultSwapDetails {
							from: from_asset,
							deposit_amount: amount,
							destination_address: EncodedAddress::from_chain_bytes(
								dst_chain.try_into().map_err(|e| {
									anyhow!("Error while parsing destination chain for solana vault swap:{}.", e)
								})?,
								dst_address.to_vec(),
							)
							.map_err(|e| {
								anyhow!("Failed to decode the destination address for solana vault swap:{}.", e)
							})?,
							to: dst_token.try_into().map_err(|e| {
								anyhow!("Error while decoding destination token for solana vault swap: {}.", e)
							})?,
							deposit_metadata,
							swap_account: vault_swap_account,
							creation_slot,
							broker_fee,
							affiliate_fees: affiliate_fees
								.into_iter()
								.map(|entry| cf_primitives::Beneficiary { account: entry.affiliate, bps: entry.fee.into() })
								.collect_vec()
								.try_into()
								.map_err(|_| {
									anyhow!("runtime supports at least as many affiliates as we allow in cf_parameters encoding")
								})?,
							refund_params,
							dca_params,
							boost_fee,
						})
					};
					(
						VaultSwapAccountAndSender {
							vault_swap_account,
							swap_sender: sender.into(),
						},
						vault_swap_details()
							.inspect_err(|e| {
								warn!("Unable to derive swap details for account `{vault_swap_account}`: {e}")
							})
							.ok(),
					)
				}
			},
		)
		.collect()
		.await;

	Ok((new_swaps, closed_accounts))
}

async fn get_changed_program_swap_accounts(
	sol_rpc: &SolRetryRpcClient,
	sc_opened_accounts: HashSet<SolAddress>,
	sc_closure_initiated_accounts: BTreeSet<VaultSwapAccountAndSender>,
	swap_endpoint_data_account_address: SolAddress,
) -> Result<(Vec<SolAddress>, Vec<VaultSwapAccountAndSender>, u64), anyhow::Error> {
	let (_historical_number_event_accounts, open_event_accounts, slot) =
		get_swap_endpoint_data(sol_rpc, swap_endpoint_data_account_address).await?;

	let new_program_swap_accounts: Vec<_> = open_event_accounts
		.iter()
		.filter(|account| {
			!sc_opened_accounts.contains(account) &&
				!sc_closure_initiated_accounts.iter().any(|x| &x.vault_swap_account == *account)
		})
		.cloned()
		.collect();
	let closed_accounts: Vec<_> = sc_closure_initiated_accounts
		.into_iter()
		.filter(|account| !open_event_accounts.contains(&account.vault_swap_account))
		.collect();

	Ok((new_program_swap_accounts, closed_accounts, slot))
}

// Query the list of opened accounts from SwapEndpointDataAccount. The Swap Endpoint program ensures
// that the list is updated atomically whenever a swap event account is opened or closed.
async fn get_swap_endpoint_data(
	sol_rpc: &SolRetryRpcClient,
	swap_endpoint_data_account_address: SolAddress,
) -> Result<(u128, HashSet<SolAddress>, u64), anyhow::Error> {
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
			let bytes = base64::engine::general_purpose::STANDARD.decode(base64_string)?;

			// 8 Discriminator + 16 Historical Number Event Accounts + 4 bytes vector length + data
			if bytes.len() < ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH + 20 {
				return Err(anyhow!("Expected account to have at least 28 bytes"));
			}

			let swap_endpoint_data_account =
				SwapEndpointDataAccount::check_and_deserialize(&bytes[..])
					.map_err(|e| anyhow!("Failed to deserialize data: {:?}", e))?;

			Ok((
				swap_endpoint_data_account.historical_number_event_accounts,
				swap_endpoint_data_account
					.open_event_accounts
					.into_iter()
					.map(|acc| acc.into())
					.collect::<HashSet<_>>(),
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
) -> anyhow::Result<Vec<anyhow::Result<(SolAddress, SwapEvent)>>> {
	let account_infos = sol_rpc
		.get_multiple_accounts(
			&program_swap_event_accounts[..],
			RpcAccountInfoConfig {
				encoding: Some(UiAccountEncoding::Base64),
				data_slice: None,
				commitment: Some(CommitmentConfig::finalized()),
				min_context_slot: Some(min_context_slot),
			},
		)
		.await
		.value;

	ensure!(
		account_infos.len() == program_swap_event_accounts.len(),
		"Number of queried accounts should match number of returned accounts."
	);

	Ok(program_swap_event_accounts
		.into_iter()
		.zip(account_infos.into_iter())
		.map(|(account, account_info)| {
			Ok((
				account,
				match account_info {
					Some(UiAccount {
						data: UiAccountData::Binary(base64_string, UiAccountEncoding::Base64),
						..
					}) => {
						let bytes = base64::engine::general_purpose::STANDARD
							.decode(base64_string)
							.map_err(|e| anyhow!("Failed to decode base64 string: {}", e))?;

						SwapEvent::check_and_deserialize(&bytes[..])
							.map_err(|e| anyhow!("Failed to deserialize data: {}", e))
					},
					Some(other) => Err(anyhow!(
						"Expected UiAccountData::Binary(_, UiAccountEncoding::Base64), got {}",
						match other.data {
							UiAccountData::Binary(_, other) =>
								format!("UiAccountData::Binary(_, {:?})", other),
							UiAccountData::Json(_) => "UiAccountData::Json(_)".to_string(),
							UiAccountData::LegacyBinary(_) =>
								"UiAccountData::LegacyBinary(_)".to_string(),
						}
					)),
					// It could happen that some account is closed between the queries. This
					// should not happen because:
					// 1. Accounts in `new_program_swap_accounts` can only be accounts that have
					//    newly been opened and they won't be closed until consensus is reached.
					// 2. If due to rpc load management the get event accounts rpc is queried at a
					//    slot < get swap endpoint data rpc slot, the min_context_slot will prevent
					//    it from being executed before that.
					// This could only happen if an engine is behind and were to see the account
					// opened and closed between queries. That's not realistic as it takes
					// minutes for an account to be closed and even if it were to happen
					// it's not problematic as we'd have reached consensus and the engine
					// would just filter it out.
					None => Err(anyhow!("Account does not exist.")),
				}
				.context(format!("Error getting SwapEvent data for account `{account}`."))?,
			))
		})
		.collect())
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
					.into_iter()
					.collect()
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
						Default::default(),
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
						Default::default(),
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
