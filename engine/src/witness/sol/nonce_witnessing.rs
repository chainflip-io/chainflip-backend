use cf_chains::sol::{SolAddress, SolHash};
use itertools::Itertools;

use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::SolRetryRpcApi,
	rpc_client_api::{
		ParsedAccount, RpcAccountInfoConfig, UiAccount, UiAccountData, UiAccountEncoding,
	},
};
use anyhow::{anyhow, Result};
use serde_json::Value;
use sol_prim::SlotNumber;
use std::str::FromStr;
pub async fn get_durable_nonce<SolRetryRpcClient>(
	sol_client: &SolRetryRpcClient,
	nonce_account: SolAddress,
	previous_slot: SlotNumber,
) -> Result<Option<(SolHash, SlotNumber)>>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let response = sol_client
		.get_multiple_accounts(
			&[nonce_account],
			RpcAccountInfoConfig {
				// Using JsonParsed will return the token accounts and the sol deposit channels
				// in a nicely parsed format. For our fetch token accounts it will return it encoded
				// as the default base64.
				encoding: Some(UiAccountEncoding::JsonParsed),
				data_slice: None,
				commitment: Some(CommitmentConfig::finalized()),
				min_context_slot: Some(previous_slot),
			},
		)
		.await;
	let account_info = response
		.value
		.into_iter()
		.exactly_one()
		.expect("We queried for exactly one account.");

	match account_info {
		Some(UiAccount {
			data: UiAccountData::Json(ParsedAccount { program, space, parsed }),
			..
		}) => {
			// Check that the program string is "nonce"
			if program != "nonce" {
				return Err(anyhow!("Expected nonce account, got program {}", program));
			}

			if space != sol_prim::consts::NONCE_ACCOUNT_LENGTH {
				return Err(anyhow!("Expected nonce account, got space {:?}", space));
			}

			let info = parsed
				.get("info")
				.and_then(Value::as_object)
				.ok_or_else(|| anyhow!("Info not found"))?;
			let hash = SolHash::from_str(
				info.get("blockhash")
					.and_then(Value::as_str)
					.ok_or_else(|| anyhow!("Blockhash not found"))?,
			)?;
			Ok(Some((hash, response.context.slot)))
		},
		Some(_) =>
			Err(anyhow!("Nonce data account encoding is not JsonParsed: {:?}", account_info)),
		None => Ok(None),
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{HttpEndpoint, NodeContainer},
		sol::retry_rpc::SolRetryRpcClient,
	};
	use cf_chains::{Chain, Solana};
	use cf_utilities::task_scope::task_scope;
	use futures::FutureExt;

	use super::*;

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_get_nonces() {
		task_scope(|scope| {
			async move {
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

				let nonce_account = get_durable_nonce(
					&retry_client,
					SolAddress::from_str("6TcAavZQgsTCGJkrxrtu8X26H7DuzMH4Y9FfXXgoyUGe").unwrap(),
					0,
				)
				.await
				.unwrap()
				.unwrap();

				println!("Durable Nonce Info: {:?}", nonce_account);
				assert_eq!(
					nonce_account.0,
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
