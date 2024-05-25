use crate::witness::common::{RuntimeCallHasChain, RuntimeHasChain};
use anyhow::ensure;
use cf_chains::{
	instances::ChainInstanceFor,
	sol::{SolAddress, SolHash, SolPubkey},
	Chain,
};
use cf_primitives::EpochIndex;
use futures_core::Future;
use sp_core::{H160, H256};

use crate::witness::common::chunked_chain_source::chunked_by_vault::deposit_addresses::Addresses;

use std::collections::BTreeMap;

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
		self.then(move |epoch, header| {
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

// // TODO: Add description
// async fn sol_ingresses_at_block<SolRetryRpcClient>(
// 	sol_rpc: &SolRetryRpcClient,
// 	addresses: Vec<SolAddress>,
// // ) -> Result<Vec<(SolAddress, u128, u128)>, anyhow::Error>
// -> Vec<Option<UiAccount>>
// )
// where
// 	SolRetryRpcClient: Send + Sync + Clone,
// {
// 	// TODO: For now we just assume that the array will contain both the deposit addresses
// 	// and the fetch accounts properly ordered.
// 	let balances = sol_rpc
// 		.get_multiple_accounts_with_config(addresses.clone(), RpcAccountInfoConfig {
// 			encoding: Some(UiAccountEncoding::JsonParsed),
// 			data_slice: None,
// 			commitment: Some(CommitmentConfig::finalized()),
// 			min_context_slot: None,
// 		},)
// 		.await.expect("Failed to get multiple accounts");

// 	ensure!(
// 		addresses.len() == balances.len()
// 	);
// 	balances
// }

async fn sol_account_infos<SolRetryRpcClient>(
	sol_rpc: &SolRetryRpcClient,
	addresses: Vec<SolPubkey>,
) -> Result<(impl Iterator<Item = (SolPubkey, u128)>, u64), anyhow::Error>
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

	// Assumption that it's intertwined sol deposit channels and fetch accounts. Maybe we will do it
	// differently passing two separate arrays. But we need to call accounts and fetch acocunts in
	// the same RPC call.
	let accounts_info =
		accounts_info.value.into_iter().enumerate().map(move |(index, account_info)| {
			match account_info {
				Some(account_info) => {
					println!("account_info {:?}", account_info);
					if index % 2 == 0 {
						println!("Parsing regular Sol deposit channel");
						// TODO: If there is account info data, we expected it to be JsonParsed. We
						// can then parse the owner and chec it's the System program. Probably not
						// really necessary though.
						(addresses[index].clone(), account_info.lamports as u128)
					} else {
						println!("Parsing Fetch account");

						// Fetch account - manually parse the data to get the cumulative fetch
						// amount
						let base64_string = match account_info.data {
							UiAccountData::Binary(base64_string, UiAccountEncoding::Base64) =>
								Some(base64_string.clone()),
							_ => None,
						};
						println!("base64_string {:?}", base64_string);
						let fetch_cumulative = base64_string
							.map(|base64_string| {
								// Decode the base64 string to bytes
								let mut bytes = base64::decode(&base64_string)
									.expect("Failed to decode base64 string");

								println!("bytes {:?}", bytes);

								// Check that there are 24 bytes (16 from u128 + 8 from
								// discriminator)
								ensure!(bytes.len() == 24);

								// Remove the first 8 bytes
								bytes.drain(..8);

								let array: [u8; 16] =
									bytes.try_into().expect("Byte slice length doesn't match u128");
								Ok(u128::from_le_bytes(array))
							})
							.expect("Got the wrong data type for fetch account data");

						(
							addresses[index].clone(),
							fetch_cumulative.expect("Failed to decode base64 string to u128"),
						)
					}
				},
				// When no account in the address
				None => (addresses[index].clone(), 0),
			}
		});

	Ok((accounts_info, slot))
}

async fn get_sol_deposit_channel_balance(account_info: UiAccount) -> u64 {
	// For now we don't really do any checks on the data
	account_info.lamports
}

async fn get_sol_fetched_balance(account_info: UiAccount) -> u64 {
	// For now we don't really do any checks on the data
	account_info.lamports
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

				let addresses = vec![
					SolPubkey::from_str("BrX9Z85BbmXYMjvvuAWU8imwsAqutVQiDg9uNfTGkzrJ").unwrap(), /* normal account */
					// TODO: This one will fail because it's not a fetch account
					SolPubkey::from_str("5WQayu3ARKuStAP3P6PvqxBfYo3crcKc2H821dLFhukz").unwrap(), /* token account */
				];

				let account_infos = sol_account_infos(&retry_client, addresses).await.unwrap();
				println!("account_infos {:?}", account_infos.0.collect::<Vec<_>>());
				println!("slot {:?}", account_infos.1);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
