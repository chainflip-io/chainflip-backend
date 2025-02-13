use cf_chains::sol::SolAddress;
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
use sol_prim::Slot;
use std::str::FromStr;

// We want to return None if the account is not found or there is any error. It should
// only error if the rpc call fails or returns an unrecognized format respons. That is
// because this address will be provided by the user (user alts) and in case of the address
// not being a valid ALT we still want to reach consensus.
#[allow(dead_code)]
pub async fn get_lookup_table_state<SolRetryRpcClient>(
	sol_rpc: &SolRetryRpcClient,
	lookup_table_address: SolAddress,
) -> Result<Option<Vec<SolAddress>>, anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let account_info = sol_rpc
		.get_multiple_accounts(
			&[lookup_table_address],
			RpcAccountInfoConfig {
				encoding: Some(UiAccountEncoding::JsonParsed),
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

	match account_info {
		Some(UiAccount {
			data: UiAccountData::Json(ParsedAccount { program, space: _, parsed }),
			owner,
			..
		}) => {
			if program != "address-lookup-table" {
				tracing::info!("Program is not an address lookup table: {}", program);
				return Ok(None);
			}

			let owner_address = SolAddress::from_str(owner.as_str()).unwrap();

			if owner_address != sol_prim::consts::ADDRESS_LOOKUP_TABLE_PROGRAM {
				tracing::info!("Owner is not address lookup table program: {}", owner);
				return Ok(None);
			}

			let info = match parsed.get("info").and_then(Value::as_object) {
				Some(value) => value,
				None => {
					tracing::info!("Failed to parse the info: {}", parsed);
					return Ok(None);
				},
			};

			let deactivation_slot: Slot = Slot::from_str(
				info.get("deactivationSlot").and_then(Value::as_str).ok_or(anyhow!(
					"Deactivation slot not found in address lookup table account info: {:?}",
					info
				))?,
			)?;

			// Address lookup table is being deactivated
			if deactivation_slot != Slot::MAX {
				return Ok(None);
			}
			let addresses = info.get("addresses").and_then(Value::as_array).ok_or(anyhow!(
				"Addresses not found in address lookup table account info: {:?}",
				info
			))?;

			let addresses_vector: Vec<SolAddress> = addresses
				.iter()
				.filter_map(|address| address.as_str()) // This will now work with the array elements
				.map(|address| SolAddress::from_str(address).unwrap())
				.collect();

			Ok(Some(addresses_vector))
		},
		Some(_) => {
			tracing::info!(
				"Address lookup table account encoding is not JsonParsed for account {:?}: {:?}",
				lookup_table_address,
				account_info
			);
			Ok(None)
		},
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
	use cf_utilities::task_scope;
	use futures_util::FutureExt;
	use std::str::FromStr;

	use super::*;

	#[tokio::test]
	#[ignore = "requires a running localnet"]
	async fn test_get_lookup_table_state() {
		task_scope::task_scope(|scope| {
			async {
				let client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: HttpEndpoint {
							// http_endpoint: "http://0.0.0.0:8899".into(),
							http_endpoint: "https://api.mainnet-beta.solana.com".into(),
						},
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				// Mainnet-beta deployed address lookup table account
				let mainnet_alt_address: SolAddress =
					SolAddress::from_str("2immgwYNHBbyVQKVGCEkgWpi53bLwWNRMB5G2nbgYV17").unwrap();
				// let localnet_alt_addres: SolAddress =
				// 	SolAddress::from_str("752wqonipWUGyz8Ss9rpPE4uwuhFDPGKUusLapSzdJeh").unwrap();

				let addresses =
					get_lookup_table_state(&client, mainnet_alt_address).await.unwrap().unwrap();

				// Check the first one just to make sure it's working
				assert_eq!(
					addresses.first().unwrap(),
					&SolAddress::from_str("11111111111111111111111111111111").unwrap()
				);

				// Test that a program will return None and not error
				let addresses = get_lookup_table_state(
					&client,
					SolAddress::from_str("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4").unwrap(),
				)
				.await
				.unwrap();
				assert_eq!(addresses, None);

				// Test a non existing address
				let addresses = get_lookup_table_state(
					&client,
					SolAddress::from_str("6UzppnNP2baug3BisB9Mb1J5t43hV1YcawUtPXHchoHS").unwrap(),
				)
				.await
				.unwrap();
				assert_eq!(addresses, None);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
