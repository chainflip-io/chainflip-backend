use crate::{
	state_chain_observer::client::{
		chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::common::{RuntimeCallHasChain, RuntimeHasChain},
};
use cf_chains::{
	sol::{SolAddress, SolHash},
	Chain,
};
use cf_primitives::EpochIndex;
use futures_core::Future;

use crate::witness::common::chunked_chain_source::chunked_by_vault::deposit_addresses::Addresses;

use super::super::common::chunked_chain_source::chunked_by_vault::{
	builder::ChunkedByVaultBuilder, ChunkedByVault,
};
use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::SolRetryRpcApi,
	rpc_client_api::{
		ParsedAccount, RpcAccountInfoConfig, UiAccount, UiAccountData, UiAccountEncoding,
	},
};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::{collections::HashMap, str::FromStr, sync::Arc};

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn witness_nonces<ProcessCall, ProcessingFut, SolRetryRpcClient, StateChainClient>(
		self,
		process_call: ProcessCall,
		sol_rpc: SolRetryRpcClient,
		state_chain_client: Arc<StateChainClient>,
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
		StateChainClient: SignedExtrinsicApi + ChainApi + StorageApi + Send + Sync + 'static,
	{
		self.then(move |epoch, header| {
			assert!(<Inner::Chain as Chain>::is_block_witness_root(header.index));
			let sol_rpc = sol_rpc.clone();
			let _process_call = process_call.clone();
			let state_chain_client = state_chain_client.clone();
			async move {
				let nonce_accounts: HashMap<SolAddress, SolHash> = state_chain_client
					.storage_map::<pallet_cf_environment::SolanaUnavailableNonceAccounts<
						state_chain_runtime::Runtime,
					>, _>(state_chain_client.latest_finalized_block().hash)
					.await
					.context("Failed to get Solana unavailable nonce accounts from SC")?;

				let nonce_addresses: Vec<SolAddress> =	nonce_accounts.keys().copied().collect();

				if !nonce_addresses.is_empty() {

					let current_nonce_accounts = get_durable_nonces(&sol_rpc, nonce_addresses).await?;

					let calls = current_nonce_accounts
						.into_iter()
						.filter_map(|(address, current_durable_nonce)| {
							if current_durable_nonce != *nonce_accounts.get(&address).expect("We just queried the nonces of the nonce accounts so the nonce account must exist") {
								let call: Box<state_chain_runtime::RuntimeCall> = Box::new(
								pallet_cf_environment::Call::<
									state_chain_runtime::Runtime
								>::update_sol_nonce { nonce_account: address, durable_nonce: current_durable_nonce }.into());

								Some(state_chain_client.finalize_signed_extrinsic(
									pallet_cf_witnesser::Call::witness_at_epoch {
										call,
										epoch_index: epoch.index,
									},
								))
							} else {
								None
							}
						})
						.collect::<Vec<_>>();

					for call in calls {
						call.await;
					}
				}
				Ok::<_, anyhow::Error>(())
			}
		})
	}
}

async fn get_durable_nonces<SolRetryRpcClient>(
	sol_client: &SolRetryRpcClient,
	nonce_accounts: Vec<SolAddress>,
) -> Result<Vec<(SolAddress, SolHash)>>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let accounts_info = sol_client
		.get_multiple_accounts(
			nonce_accounts.clone().as_slice(),
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
		.await
		.value;

	let mut result = Vec::new();

	for (nonce_account, nonce_account_info) in nonce_accounts.iter().zip(accounts_info) {
		if let Some(UiAccount { data: UiAccountData::Json(account_data), .. }) = nonce_account_info
		{
			let ParsedAccount { program, space, parsed } = account_data;
			// Check that the program string is "nonce"
			if program != "nonce" {
				return Err(anyhow!("Expected nonce account, got program {}", program));
			}

			if space != sol_prim::consts::NONCE_ACCOUNT_LENGTH {
				return Err(anyhow!("Expected nonce account, got space {:?}", space));
			}

			let info =
				parsed.get("info").and_then(Value::as_object).ok_or(anyhow!("Info not found"))?;
			let hash = SolHash::from_str(
				info.get("blockhash")
					.and_then(Value::as_str)
					.ok_or(anyhow!("Blockhash not found"))?,
			)?;

			result.push((*nonce_account, hash));
		} else {
			return Err(anyhow!("Expected UiAccountData::Json(ParsedAccount)"));
		}
	}
	Ok(result)
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{NodeContainer, WsHttpEndpoints},
		sol::retry_rpc::SolRetryRpcClient,
	};
	use cf_chains::{Chain, Solana};
	use futures::FutureExt;
	use utilities::task_scope::task_scope;

	use super::*;

	#[tokio::test]
	async fn test_get_nonces() {
		task_scope(|scope| {
			async move {
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

				let nonce_accounts = get_durable_nonces(
					&retry_client,
					vec![SolAddress::from_str("6TcAavZQgsTCGJkrxrtu8X26H7DuzMH4Y9FfXXgoyUGe")
						.unwrap()],
				)
				.await
				.unwrap();
				let nonce_account = nonce_accounts.first().unwrap();

				println!("Durable Nonce Info: {:?}", nonce_account);
				assert_eq!(
					nonce_account.0,
					SolAddress::from_str("6TcAavZQgsTCGJkrxrtu8X26H7DuzMH4Y9FfXXgoyUGe").unwrap(),
				);
				assert_eq!(
					nonce_account.1,
					SolHash::from_str("F9X2sMsGGJUGrVPs42vQc3fyi9rGqd7NFUWKT8SQTkCW").unwrap(),
				);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap()
	}
}
