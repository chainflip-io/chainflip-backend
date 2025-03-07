use cf_chains::sol::{api::AltConsensusResult, SolAddress};

use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::SolRetryRpcApi,
	rpc_client_api::{
		ParsedAccount, RpcAccountInfoConfig, UiAccount, UiAccountData, UiAccountEncoding,
	},
};
use anyhow::{anyhow, Result};
use serde_json::Value;
use sol_prim::{AddressLookupTableAccount, Slot};
use std::str::FromStr;

// We want to return None if the account is not found or there is any error. It should
// only error if the rpc call fails or returns an unrecognized format respons. That is
// because this address will be provided by the user (user alts) and in case of the address
// not being a valid ALT we still want to reach consensus.
pub async fn get_lookup_table_state<SolRetryRpcClient>(
	sol_rpc: &SolRetryRpcClient,
	lookup_table_addresses: Vec<SolAddress>,
) -> Result<AltConsensusResult<Vec<AddressLookupTableAccount>>, anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	sol_rpc
		.get_multiple_accounts(
			&lookup_table_addresses[..],
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
		.zip(lookup_table_addresses)
		.map(|(account_info, lookup_table_address)| {
			parse_alt_account_info(account_info, lookup_table_address)
		})
		.collect::<Result<Option<_>, _>>()
		.map(|maybe_consensus_alts| match maybe_consensus_alts {
			Some(alts) => AltConsensusResult::ValidConsensusAlts(alts),
			None => AltConsensusResult::AltsInvalidNoConsensus,
		})
}
fn parse_alt_account_info(
	account_info: Option<UiAccount>,
	lookup_table_address: SolAddress,
) -> Result<Option<AddressLookupTableAccount>, anyhow::Error> {
					if owner_address != sol_prim::consts::ADDRESS_LOOKUP_TABLE_PROGRAM {
						tracing::info!("Owner is not address lookup table program: {}", owner);
						return Ok(None);
					}

			let owner_address = SolAddress::from_str(owner.as_str())?;

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

					Ok(Some(AddressLookupTableAccount {
						key: lookup_table_address.into(),
						addresses: info
							.get("addresses")
							.and_then(Value::as_array)
							.ok_or(anyhow!(
								"Addresses not found in address lookup table account info: {:?}",
								info
							))?
							.iter()
							.filter_map(|address| address.as_str())
							// if any of the address in the lookup table cannot be parsed (which
							// means its invalid), we currently fail the whole table, and hence the
							// whole vote. We could return a table with missing addresses but then
							// we would have to change the AddressLookupTableAccount account type
							// (to have Option<Vec<Addresses>>). Since its a type taken from the
							// Solana sdk, we dont want to modify it. Hence, we fail here.
							.map(|address| SolAddress::from_str(address).map(|a| a.into()))
							.collect::<Result<_, _>>()?,
					}))
				},
				// If the account is not JsonParsed as a Lookup Table we assume it's either empty or
				// another account. We can also consider not returning an Option and instead
				// return an empty vector if the ALT is not found.
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

			Ok(Some(AddressLookupTableAccount {
				key: lookup_table_address.into(),
				addresses: info
					.get("addresses")
					.and_then(Value::as_array)
					.ok_or(anyhow!(
						"Addresses not found in address lookup table account info: {:?}",
						info
					))?
					.iter()
					.filter_map(|address| address.as_str())
					// if any of the address in the lookup table cannot be parsed (which
					// means its invalid), we currently fail the whole table, and hence the
					// whole vote. We could return a table with missing addresses but then
					// we would have to change the AddressLookupTableAccount account type
					// (to have Option<Vec<Addresses>>). Since its a type taken from the
					// Solana sdk, we dont want to modify it. Hence, we fail here.
					.map(|address| SolAddress::from_str(address).map(|a| a.into()))
					.collect::<Result<_, _>>()?,
			}))
		},
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
	use serde_json::json;
	use std::str::FromStr;

	use super::*;

	#[test]
	fn test_parse_alt_account_info() {
		// Using Account info from a real mainnet ALT
		let addresses_list = &[
			"11111111111111111111111111111111",
			"ComputeBudget111111111111111111111111111111",
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
			"TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
			"Memo1UhkJRfHyvLMcVucJwxXeuD728EqVDDwQDxFMNo",
			"SysvarRent111111111111111111111111111111111",
			"SysvarC1ock11111111111111111111111111111111",
			"ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
			"metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",
			"EUqojwWA2rd19FZrzeBncJsm38Jm1hEhE3zsmX3bRc2o",
			"9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin",
			"srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
			"RVKd61ztZW9GUwhRbbLoYVRE5Xf1B2tVscKqwZqXgEr",
			"27haf8L6oxUeXrHrgEgsexjSY5hbVUWEmvv9Nyxg8vQv",
			"675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
			"5quBtoiQqxF9Jv6KYKctB59NT3gtJD2Y65kdnB1Uev3h",
			"CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
			"routeUGWgWzqBWFcrCfv8tritsqukccJPu3q5GPP3xS",
			"EhhTKczWMGQt46ynNeRX1WfeagwwJd7ufHvCDjRxjo5Q",
			"CBuCnLe26faBpcBP2fktp4rp8abpcAnTWft6ZrP5Q4T",
			"9KEPoZmtHUrBbhWN1v1KWLMkkvwY6WLtAVUCPRtRjP4z",
			"6FJon3QE27qgPVggARueB22hLvoh22VzJpXv4rBEoSLF",
			"CC12se5To1CdEuw7fDS27B7Geo5jJyL7t5UK2B44NgiH",
			"9HzJyW1qZsEiSfMUf6L2jo3CcTKAyBmSyKdwQeYisHrC",
		];

		let alt_account = parse_alt_account_info(
			Some(UiAccount {
				lamports: 180602362,
				data: UiAccountData::Json(ParsedAccount {
					program: "address-lookup-table".to_string(),
					parsed: json!({
						"info": {
							"addresses": addresses_list,
							"authority": "RayZuc5vEK174xfgNFdD9YADqbbwbFjVjY4NM8itSF9",
							"deactivationSlot": "18446744073709551615",
							"lastExtendedSlot": "209620855",
							"lastExtendedSlotStartIndex": 0
						},
						"type": "lookupTable"
					}),
					space: 824,
				}),
				owner: "AddressLookupTab1e1111111111111111111111111".to_string(),
				executable: false,
				rent_epoch: 18446744073709551615,
				space: Some(824),
			}),
			SolAddress::from_str("2immgwYNHBbyVQKVGCEkgWpi53bLwWNRMB5G2nbgYV17").unwrap(),
		)
		.unwrap()
		.unwrap();

		assert!(
			alt_account.key ==
				SolAddress::from_str("2immgwYNHBbyVQKVGCEkgWpi53bLwWNRMB5G2nbgYV17")
					.unwrap()
					.into()
		);
		let addresses = alt_account.addresses;
		let expected_addresses = addresses_list
			.iter()
			.map(|s| SolAddress::from_str(s).unwrap().into())
			.collect::<Vec<_>>();

		assert_eq!(
			addresses, expected_addresses,
			"The parsed addresses do not match the expected addresses"
		);

		// Test the expiry slot
		let expiring_alt_info = parse_alt_account_info(
			Some(UiAccount {
				lamports: 180602362,
				data: UiAccountData::Json(ParsedAccount {
					program: "address-lookup-table".to_string(),
					parsed: json!({
						"info": {
							"addresses": addresses_list,
							"authority": "RayZuc5vEK174xfgNFdD9YADqbbwbFjVjY4NM8itSF9",
							"deactivationSlot": "18446744073709551614",
							"lastExtendedSlot": "209620855",
							"lastExtendedSlotStartIndex": 0
						},
						"type": "lookupTable"
					}),
					space: 824,
				}),
				owner: "AddressLookupTab1e1111111111111111111111111".to_string(),
				executable: false,
				rent_epoch: 18446744073709551615,
				space: Some(824),
			}),
			SolAddress::from_str("2immgwYNHBbyVQKVGCEkgWpi53bLwWNRMB5G2nbgYV17").unwrap(),
		)
		.unwrap();
		assert!(expiring_alt_info.is_none());

		// Test that a program will return None and not error
		let program_account = parse_alt_account_info(
			Some(UiAccount {
				lamports: 1141440,
				data: UiAccountData::Json(ParsedAccount {
					program: "bpf-upgradeable-loader".to_string(),
					parsed: json!({
						"info": {
							"programData": "4Ec7ZxZS6Sbdg5UGSLHbAnM7GQHp2eFd4KYWRexAipQT"
						},
						"type": "program"
					}),
					space: 36,
				}),
				owner: "BPFLoaderUpgradeab1e11111111111111111111111".to_string(),
				executable: true,
				rent_epoch: 18446744073709551615,
				space: Some(36),
			}),
			SolAddress::from_str("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4").unwrap(),
		)
		.unwrap();
		assert!(program_account.is_none());

		// Test a non existing address
		let empty_account = parse_alt_account_info(
			None,
			SolAddress::from_str("6UzppnNP2baug3BisB9Mb1J5t43hV1YcawUtPXHchoHS").unwrap(),
		)
		.unwrap();
		assert!(empty_account.is_none());
	}

	#[tokio::test]
	#[ignore = "requires an external endpoint"]
	async fn test_get_non_lookup_table() {
		task_scope::task_scope(|scope| {
			async {
				let client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: HttpEndpoint {
							http_endpoint: "https://api.mainnet-beta.solana.com".into(),
						},
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let mainnet_empty_address: SolAddress =
					SolAddress::from_str("ASriuNGwqUosyrUYNrpjMNUsGYAKFAVB4e3bVpeaRC7Y").unwrap();

				let addresses =
					get_lookup_table_state(&client, vec![mainnet_empty_address]).await.unwrap();

				assert_eq!(addresses, AltConsensusResult::AltsInvalidNoConsensus);

				let mainnet_nonce_account: SolAddress =
					SolAddress::from_str("3bVqyf58hQHsxbjnqnSkopnoyEHB9v9KQwhZj7h1DucW").unwrap();

				let addresses =
					get_lookup_table_state(&client, vec![mainnet_nonce_account]).await.unwrap();

				assert_eq!(addresses, AltConsensusResult::AltsInvalidNoConsensus);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
