#![cfg(feature = "integration-test")]

//! This tests integration with the KeyManager contract
//! For instruction on how to run this test, see `engine/tests/README.md`

use chainflip_engine::{
	eth::key_manager::{ChainflipKey, KeyManager, KeyManagerEvent},
	logging::utils,
};

use std::str::FromStr;
use web3::types::{H160, H256, U256};

mod common;
use crate::common::IntegrationTestConfig;

#[cfg(feature = "integration-test")]
#[tokio::test]
pub async fn test_all_key_manager_events() {
	let root_logger = utils::new_cli_logger();

	let integration_test_config = IntegrationTestConfig::from_file("tests/config.toml").unwrap();

	let km_events = common::get_contract_events(
		KeyManager::new(integration_test_config.eth.key_manager_address),
		root_logger,
	)
	.await;

	// The following event details correspond to the events in
	// chainflip-eth-contracts/scripts/deploy_and.py All the key strings in this test are decimal
	// pub keys derived from the priv keys in the consts.py script https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py
	km_events
		.iter()
		.find(|event| match &event.event_parameters {
			KeyManagerEvent::AggKeyNonceConsumersSet { addrs } => {
				assert_eq!(
					addrs,
					&vec![
						H160::from_str("0xe7f1725e7734ce288f8367e1bb143e90bb3f0512").unwrap(),
						H160::from_str("0x9fe46736679d2d9a65f0992f2272de9f3c7fa6e0").unwrap(),
						H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9").unwrap()
					]
				);
				true
			},
			_ => false,
		})
		.expect("Didn't find AggKeyNonceConsumersSet event");

	km_events
		.iter()
		.find(|event| match &event.event_parameters {
			KeyManagerEvent::AggKeyNonceConsumersUpdated { new_addrs } => {
				assert_eq!(
					new_addrs,
					&vec![
						H160::from_str("0xe7f1725e7734ce288f8367e1bb143e90bb3f0512").unwrap(),
						H160::from_str("0x9fe46736679d2d9a65f0992f2272de9f3c7fa6e0").unwrap(),
						H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9").unwrap()
					]
				);
				true
			},
			_ => false,
		})
		.expect("Didn't find AggKeyNonceConsumersUpdated event");

	km_events
            .iter()
            .find(|event| match &event.event_parameters {
            KeyManagerEvent::AggKeySetByAggKey {
                old_agg_key, new_agg_key
            } => {
                assert_eq!(old_agg_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                assert_eq!(new_agg_key,&ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                true
            },
            _ => false,
        }).expect("Didn't find AggKeySetByAggKey event");

	km_events
        .iter()
        .find(|event| match &event.event_parameters {
            KeyManagerEvent::AggKeySetByGovKey {
                old_agg_key, new_agg_key
            } => {
                if old_agg_key == &ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap()
                || old_agg_key == &ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap(){
                    assert_eq!(new_agg_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                }else{
                    panic!("Unexpected AggKeySetByGovKey event. The details did not match the 2 expected AggKeySetByGovKey events");
                }
                true
            },
            _ => false,
        }).expect("Didn't find AggKeySetByGovKey event");

	km_events
		.iter()
		.find(|event| match &event.event_parameters {
			KeyManagerEvent::CommKeySetByAggKey { old_comm_key, new_comm_key } => {
				assert_eq!(
					old_comm_key,
					&H160::from_str("0x14dc79964da2c08b23698b3d3cc7ca32193d9955").unwrap()
				);
				assert_eq!(
					new_comm_key,
					&H160::from_str("0x976ea74026e726554db657fa54763abd0c3a0aa9").unwrap()
				);
				true
			},
			_ => false,
		})
		.expect("Didn't find CommKeySetByAggKey event");

	km_events
		.iter()
		.find(|event| match &event.event_parameters {
			KeyManagerEvent::CommKeySetByCommKey { old_comm_key, new_comm_key } => {
				assert_eq!(
					old_comm_key,
					&H160::from_str("0x976ea74026e726554db657fa54763abd0c3a0aa9").unwrap()
				);
				assert_eq!(
					new_comm_key,
					&H160::from_str("0x14dc79964da2c08b23698b3d3cc7ca32193d9955").unwrap()
				);
				true
			},
			_ => false,
		})
		.expect("Didn't find CommKeySetByCommKey event");

	km_events
		.iter()
		.find(|event| match &event.event_parameters {
			KeyManagerEvent::GovKeySetByAggKey { old_gov_key, new_gov_key } => {
				assert_eq!(
					old_gov_key,
					&H160::from_str("0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc").unwrap()
				);
				assert_eq!(
					new_gov_key,
					&H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
				);
				true
			},
			_ => false,
		})
		.expect("Didn't find GovKeySetByAggKey event");

	km_events
		.iter()
		.find(|event| match &event.event_parameters {
			KeyManagerEvent::GovKeySetByGovKey { old_gov_key, new_gov_key } => {
				assert_eq!(
					old_gov_key,
					&H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
				);
				assert_eq!(
					new_gov_key,
					&H160::from_str("0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc").unwrap()
				);
				true
			},
			_ => false,
		})
		.expect("Didn't find GovKeySetByGovKey event");

	km_events
		.iter()
		.find(|event| match &event.event_parameters {
			KeyManagerEvent::SignatureAccepted { sig_data, signer } => {
				assert_eq!(sig_data.key_man_addr, integration_test_config.eth.key_manager_address);
				assert_eq!(sig_data.chain_id, U256::from_dec_str("31337").unwrap());
				assert_eq!(sig_data.nonce, U256::from_dec_str("0").unwrap());
				assert_eq!(
					signer,
					&H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap()
				);
				true
			},
			_ => false,
		})
		.expect("Didn't find SignatureAccepted event");

	km_events
		.iter()
		.find(|event| match &event.event_parameters {
			KeyManagerEvent::GovernanceAction { message } => {
				assert_eq!(message, &H256::from_low_u64_be(42069).as_ref());
				true
			},
			_ => false,
		})
		.expect("Didn't find GovernanceAction event");
}
