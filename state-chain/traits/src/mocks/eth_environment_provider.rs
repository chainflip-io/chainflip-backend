use cf_primitives::{Asset, EthereumAddress};

/// A mock that just returns some constants for the KeyManager and Chain ID.
pub struct MockEthEnvironmentProvider;

impl crate::EthEnvironmentProvider for MockEthEnvironmentProvider {
	fn token_address(_asset: Asset) -> Option<EthereumAddress> {
		Some([0xcf; 20])
	}
	fn key_manager_address() -> EthereumAddress {
		[0xcf; 20]
	}
	fn vault_address() -> EthereumAddress {
		[0xcf; 20]
	}
	fn stake_manager_address() -> EthereumAddress {
		[0xcf; 20]
	}
	fn chain_id() -> u64 {
		42
	}
}
