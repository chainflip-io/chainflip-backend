#![cfg(feature = "integration-test")]

mod common;

use std::str::FromStr;

use crate::common::IntegrationTestConfig;
use chainflip_engine::eth::{
	event::Event,
	vault::{Vault, VaultEvent},
};
use web3::types::Bytes;

fn event_is_present(events: &[Event<VaultEvent>], vault_event: VaultEvent) -> bool {
	events.iter().any(|event| event.event_parameters == vault_event)
}

#[tokio::test]
pub async fn test_all_vault_events() {
	let integration_test_config = IntegrationTestConfig::from_file("tests/config.toml").unwrap();

	let vault_events =
		common::get_contract_events(Vault::new(integration_test_config.eth.vault_address)).await;

	assert!(event_is_present(
		&vault_events,
		VaultEvent::SwapNative {
			destination_chain: 42069,
			destination_address: Bytes(vec![164, 85]),
			destination_token: 1,
			amount: 10u128.pow(17),
			sender: web3::types::H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266")
				.unwrap(),
		}
	));

	assert!(event_is_present(
		&vault_events,
		VaultEvent::SwapToken {
			destination_chain: 42069,
			destination_address: Bytes(vec![164, 85]),
			destination_token: 1,
			amount: 10u128.pow(17),
			sender: web3::types::H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266")
				.unwrap(),
			source_token: web3::types::H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9")
				.unwrap(),
		}
	));

	assert!(event_is_present(
		&vault_events,
		VaultEvent::CommunityGuardDisabled { community_guard_disabled: true }
	));

	assert!(event_is_present(
		&vault_events,
		VaultEvent::CommunityGuardDisabled { community_guard_disabled: false }
	));

	assert!(event_is_present(&vault_events, VaultEvent::Suspended { suspended: true }));

	assert!(event_is_present(&vault_events, VaultEvent::Suspended { suspended: false }));

	assert!(event_is_present(
		&vault_events,
		VaultEvent::XCallNative {
			destination_chain: 42069,
			destination_address: Bytes(vec![164, 85]),
			destination_token: 3,
			amount: 10u128.pow(17),
			sender: web3::types::H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266")
				.unwrap(),
			message: Bytes(vec![164, 85]),
			gas_amount: 42069,
			refund_address: Bytes(vec![164, 85]),
		}
	));

	assert!(event_is_present(
		&vault_events,
		VaultEvent::XCallToken {
			destination_chain: 42069,
			destination_address: Bytes(vec![164, 85]),
			destination_token: 3,
			amount: 10u128.pow(17),
			sender: web3::types::H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266")
				.unwrap(),
			message: Bytes(vec![164, 85]),
			gas_amount: 42069,
			refund_address: Bytes(vec![164, 85]),
			source_token: web3::types::H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9")
				.unwrap(),
		}
	));

	let swap_id = [
		0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		164, 85,
	];

	assert!(event_is_present(
		&vault_events,
		VaultEvent::AddGasNative { swap_id, amount: 10u128.pow(17) }
	));

	assert!(event_is_present(
		&vault_events,
		VaultEvent::AddGasToken {
			swap_id,
			amount: 10u128.pow(17),
			token: web3::types::H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9")
				.unwrap(),
		}
	));

	assert!(event_is_present(
		&vault_events,
		VaultEvent::TransferNativeFailed {
			recipient: web3::types::H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9")
				.unwrap(),
			amount: 10u128.pow(17),
		}
	));

	assert!(event_is_present(
		&vault_events,
		VaultEvent::TransferTokenFailed {
			recipient: web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
				.unwrap(),
			amount: 300000000000000001,
			token: web3::types::H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9")
				.unwrap(),
			reason: Bytes(
				[
					8u8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 38, 69, 82, 67, 50, 48, 58,
					32, 116, 114, 97, 110, 115, 102, 101, 114, 32, 97, 109, 111, 117, 110, 116, 32,
					101, 120, 99, 101, 101, 100, 115, 32, 98, 97, 108, 97, 110, 99, 101, 0, 0, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
				]
				.to_vec()
			)
		}
	));

	assert!(event_is_present(
		&vault_events,
		VaultEvent::UpdatedKeyManager {
			key_manager: web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
				.unwrap()
		}
	));
}
