use cf_chains::eth::{api::EthEnvironmentProvider, ethabi::Address};

/// A mock that just returns defaults for the KeyManager and Chain ID.
pub struct MockEthEnvironment;

impl EthEnvironmentProvider for MockEthEnvironment {
	fn contract_address(_contract: cf_chains::eth::api::EthereumContract) -> Address {
		Default::default()
	}

	fn next_nonce() -> u64 {
		Default::default()
	}

	fn token_address(_asset: cf_primitives::chains::assets::eth::Asset) -> Option<Address> {
		Some(Default::default())
	}

	fn chain_id() -> cf_chains::eth::api::EthereumChainId {
		Default::default()
	}
}
