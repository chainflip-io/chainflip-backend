#![cfg(feature = "integration-test")]

//! This tests integration with the StateChainGateway contract
//! For instruction on how to run this test, see `engine/tests/README.md`

use chainflip_engine::eth::state_chain_gateway::{StateChainGateway, StateChainGatewayEvent};

use sp_runtime::AccountId32;
use std::str::FromStr;
use web3::types::{H160, U256};

mod common;
use crate::common::IntegrationTestConfig;

#[tokio::test]
pub async fn test_all_state_chain_gateway_events() {
	let integration_test_config = IntegrationTestConfig::from_file("tests/config.toml").unwrap();

	let sm_events = common::get_contract_events(StateChainGateway::new(
		integration_test_config.eth.state_chain_gateway_address,
	))
	.await;

	// The following event details correspond to the events in
	// chainflip-eth-contracts/scripts/deploy_and.py
	sm_events
		.iter()
		.find(|event| match &event.event_parameters {
			StateChainGatewayEvent::Funded { account_id, amount, funder, return_addr } => {
				assert_eq!(
					account_id,
					&AccountId32::from_str(
						"000000000000000000000000000000000000000000000000000000000000a455"
					)
					.unwrap()
				);
				assert_eq!(amount, &1000000000000000000000u128);
				assert_eq!(
					funder,
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
		.expect("Didn't find the Funded event");

	sm_events
		.iter()
		.find(|event| match &event.event_parameters {
			StateChainGatewayEvent::RedemptionRegistered {
				account_id,
				amount,
				funder,
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
					funder,
					&web3::types::H160::from_str("0x70997970c51812dc3a010c7d01b50e0d17dc79c8")
						.unwrap()
				);
				assert!(start_time > &U256::from_str("0").unwrap());
				assert!(expiry_time > start_time);
				true
			},
			_ => false,
		})
		.expect("Didn't find the RedemptionRegistered event");

	sm_events
		.iter()
		.find(|event| match &event.event_parameters {
			StateChainGatewayEvent::RedemptionExecuted { account_id, amount, .. } => {
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
		.expect("Didn't find the RedemptionExecuted event");

	sm_events
		.iter()
		.find(|event| match &event.event_parameters {
			StateChainGatewayEvent::RedemptionExpired { account_id, amount } => {
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
		.expect("Didn't find the RedemptionExpired event");

	sm_events
		.iter()
		.find(|event| match event.event_parameters {
			StateChainGatewayEvent::MinFundingChanged { old_min_funding, new_min_funding } => {
				assert_eq!(old_min_funding, U256::from_dec_str("1000000000000000000000").unwrap());
				assert_eq!(new_min_funding, U256::from_dec_str("333333333333333311488").unwrap());
				true
			},
			_ => false,
		})
		.expect("Didn't find the MinFundingChanged event");

	sm_events
		.iter()
		.find(|event| match &event.event_parameters {
			StateChainGatewayEvent::GovernanceWithdrawal { to, amount, .. } => {
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
