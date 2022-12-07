use cf_primitives::EthereumAddress;

/// A mock that just returns some constants for the KeyManager and Chain ID.
pub struct MockEthEnvironmentProvider;

impl crate::EthEnvironmentProvider for MockEthEnvironmentProvider {
	fn flip_token_address() -> EthereumAddress {
		[0xcf; 20]
	}
	fn usdc_token_address() -> EthereumAddress {
		[0xcd; 20]
	}
	fn key_manager_address() -> EthereumAddress {
		[0xcf; 20]
	}
	fn eth_vault_address() -> EthereumAddress {
		[0xcf; 20]
	}
	fn stake_manager_address() -> EthereumAddress {
		[0xcf; 20]
	}
	fn chain_id() -> u64 {
		42
	}
}
