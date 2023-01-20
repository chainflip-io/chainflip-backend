#![cfg(feature = "integration-test")]

//! This tests integration with the StakeManager contract
//! For instruction on how to run this test, see `engine/tests/README.md`

use chainflip_engine::{
	eth::stake_manager::{StakeManager, StakeManagerEvent},
	logging::utils,
};

use sp_runtime::AccountId32;
use std::str::FromStr;
use web3::types::{H160, U256};

mod common;
use crate::common::IntegrationTestConfig;

#[tokio::test]
pub async fn test_all_stake_manager_events() {
	let root_logger = utils::new_cli_logger();

	let integration_test_config = IntegrationTestConfig::from_file("tests/config.toml").unwrap();

	let sm_events = common::get_contract_events(
		StakeManager::new(integration_test_config.eth.stake_manager_address),
		root_logger,
	)
	.await;

	// The following event details correspond to the events in
	// chainflip-eth-contracts/scripts/deploy_and.py
	sm_events
		.iter()
		.find(|event| match &event.event_parameters {
			StakeManagerEvent::Staked { account_id, amount, staker, return_addr } => {
				assert_eq!(
					account_id,
					&AccountId32::from_str(
						"000000000000000000000000000000000000000000000000000000000000a455"
					)
					.unwrap()
				);
				assert_eq!(amount, &1000000000000000000000u128);
				assert_eq!(
					staker,
					&web3::types::H160::from_str("0x70997970c51812dc3a010c7d01b50e0d17dc79c8")
						.unwrap()
				);
				assert_eq!(
					return_addr,
					&web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
						.unwrap()
				);
				true
			},
			_ => false,
		})
		.expect("Didn't find the Staked event");

	sm_events
		.iter()
		.find(|event| match &event.event_parameters {
			StakeManagerEvent::ClaimRegistered {
				account_id,
				amount,
				staker,
				start_time,
				expiry_time,
			} => {
				assert_eq!(
					account_id,
					&AccountId32::from_str(
						"000000000000000000000000000000000000000000000000000000000000a455"
					)
					.unwrap()
				);
				assert_eq!(amount, &U256::from_dec_str("333333333333333311488").unwrap());
				assert_eq!(
					staker,
					&web3::types::H160::from_str("0x70997970c51812dc3a010c7d01b50e0d17dc79c8")
						.unwrap()
				);
				assert!(start_time > &U256::from_str("0").unwrap());
				assert!(expiry_time > start_time);
				true
			},
			_ => false,
		})
		.expect("Didn't find the ClaimRegistered event");

	sm_events
		.iter()
		.find(|event| match &event.event_parameters {
			StakeManagerEvent::ClaimExecuted { account_id, amount, .. } => {
				assert_eq!(
					account_id,
					&AccountId32::from_str(
						"000000000000000000000000000000000000000000000000000000000000a455"
					)
					.unwrap()
				);
				assert_eq!(amount, &333333333333333311488);
				true
			},
			_ => false,
		})
		.expect("Didn't find the ClaimExecuted event");

	sm_events
		.iter()
		.find(|event| match event.event_parameters {
			StakeManagerEvent::MinStakeChanged { old_min_stake, new_min_stake } => {
				assert_eq!(old_min_stake, U256::from_dec_str("1000000000000000000000").unwrap());
				assert_eq!(new_min_stake, U256::from_dec_str("333333333333333311488").unwrap());
				true
			},
			_ => false,
		})
		.expect("Didn't find the MinStakeChanged event");

	sm_events
		.iter()
		.find(|event| match &event.event_parameters {
			StakeManagerEvent::GovernanceWithdrawal { to, amount, .. } => {
				assert_eq!(
					to,
					&H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
				);
				assert_eq!(amount, &250666666666666666688512);
				true
			},
			_ => false,
		})
		.expect("Didn't find the GovernanceWithdrawal event");
}
