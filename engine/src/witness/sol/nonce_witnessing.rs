use crate::witness::common::{RuntimeCallHasChain, RuntimeHasChain};
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
use crate::{
	sol::{
		commitment_config::CommitmentConfig,
		retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
		rpc_client_api::{
			ParsedAccount, RpcAccountInfoConfig, UiAccount, UiAccountData, UiAccountEncoding,
		},
	},
	// witness::common::chain_source::Header,
};
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::str::FromStr;
use tracing::{info, trace};

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	/// Witnessing the state of the nonce accounts periodically. It could also be done
	/// only when a broadcast is witnessed but this is more reliable.
	pub async fn witness_nonces<ProcessCall, ProcessingFut, SolRetryRpcClient>(
		self,
		process_call: ProcessCall,
		sol_rpc: SolRetryRpcClient,
		nonce_accounts: Vec<(SolAddress, SolHash)>,
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
		println!("DEBUGNONCES Witnessing Nonce Accounts");

		self.then(move |_epoch, header| {
			assert!(<Inner::Chain as Chain>::is_block_witness_root(header.index));
			let sol_rpc = sol_rpc.clone();
			let _process_call = process_call.clone();
			let nonce_accounts = nonce_accounts.clone();
			println!("DEBUGNONCES Witnessing Nonce Accounts Inner");
			async move {
				let nonce_addresses: Vec<SolAddress> = nonce_accounts
					.clone()
					.into_iter()
					.map(|(nonce_address, _)| nonce_address)
					.collect();
				println!("DEBUGNONCES Witnessing Nonce Accounts: {:?}", nonce_addresses);
				let current_nonce_accounts = get_durable_nonces(&sol_rpc, nonce_addresses).await?;

				// Create a vector of the nonce accounts that have changed
				let nonce_differences: Vec<(SolAddress, SolHash, SolHash)> = nonce_accounts
					.into_iter()
					.zip(current_nonce_accounts.into_iter())
					.filter_map(
						|((nonce_address, current_durable_nonce), (_, new_durable_nonce))| {
							println!("DEBUGNONCES Nonce address: {:?}", nonce_address);
							println!(
								"DEBUGNONCES current_durable_nonce: {:?}",
								current_durable_nonce
							);
							println!("DEBUGNONCES new_durable_nonce: {:?}", new_durable_nonce);
							// Assumption that the nonce accounts will match
							if current_durable_nonce != new_durable_nonce {
								Some((nonce_address, current_durable_nonce, new_durable_nonce))
							} else {
								println!("DEBUGNONCES Nonce haven't changed!");
								None
							}
						},
					)
					.collect();

				// check if nonce_differences is empty
				if !nonce_differences.is_empty() {
					// TODO: Submit an extrinsic with the new nonce hash
					println!("DEBUGNONCES Nonce differences: {:?}", nonce_differences);
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

	println!("Accounts_info: {:?}", accounts_info);

	for (nonce_account, nonce_account_info) in nonce_accounts.iter().zip(accounts_info) {
		if let Some(UiAccount { data: UiAccountData::Json(account_data), .. }) = nonce_account_info
		{
			let ParsedAccount { program, space, parsed } = account_data;
			// Check that the program string is "nonce"
			if program != "nonce" {
				return Err(anyhow!("Expected nonce account, got program {}", program));
			}
			// check that the space is 80
			if space != 80 {
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
		// use settings:: Settings
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
				// let settings = Settings::new_test().unwrap();

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

				// Nonce account
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
