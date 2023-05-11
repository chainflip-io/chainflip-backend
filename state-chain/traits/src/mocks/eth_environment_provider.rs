use cf_chains::eth::Address;
use cf_primitives::chains::assets::eth::Asset;

/// A mock that just returns some constants for the KeyManager and Chain ID.
pub struct MockEthEnvironmentProvider;

impl cf_chains::EthEnvironmentProvider for MockEthEnvironmentProvider {
	fn token_address(_asset: Asset) -> Option<Address> {
		Some([0xcf; 20].into())
	}
	fn key_manager_address() -> Address {
		[0xcf; 20].into()
	}
	fn vault_address() -> Address {
		[0xcf; 20].into()
	}
	fn state_chain_gateway_address() -> Address {
		[0xcf; 20].into()
	}
	fn chain_id() -> u64 {
		42
	}
}
