use std::str::FromStr;

use crate::{
	db::PersistentKeyDB,
	retrier::{Attempt, RequestLog, RetrierClient},
	settings::{NodeContainer, WsHttpEndpoints},
	sol::{
		commitment_config::CommitmentConfig,
		retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
		rpc_client_api::{
			ParsedAccount, RpcAccountInfoConfig, RpcTransactionConfig,
			TransactionConfirmationStatus, UiAccount, UiAccountData, UiAccountEncoding,
		},
	},
	witness::common::chain_source::{ChainClient, Header},
};
use cf_chains::sol::{SolAddress, SolHash};

use anyhow::{anyhow, Result};
use cf_chains::Solana;
use serde_json::Value;

async fn get_durable_nonces(
	sol_client: &SolRetryRpcClient,
	nonce_accounts: Vec<SolAddress>,
) -> Result<Vec<(SolAddress, SolHash)>> {
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
			// check that the space is Some(80)
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
	// use crate::settings::Settings;
	use cf_chains::Chain;
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
