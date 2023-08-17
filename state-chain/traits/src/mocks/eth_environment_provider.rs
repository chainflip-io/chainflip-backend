#![cfg(debug_assertions)]

use cf_chains::{
	eth::Address,
	evm::{EthereumChainId, EthereumContract, EvmEnvironmentProvider},
	mocks::MockEthereum,
};

/// A mock that just returns defaults for the KeyManager and Chain ID.
pub struct MockEthEnvironment;

impl EvmEnvironmentProvider<MockEthereum> for MockEthEnvironment {
	fn contract_address(_contract: EthereumContract) -> Address {
		Default::default()
	}

	fn next_nonce() -> u64 {
		Default::default()
	}

	fn token_address(_asset: cf_primitives::chains::assets::eth::Asset) -> Option<Address> {
		Some(Default::default())
	}

	fn chain_id() -> EthereumChainId {
		Default::default()
	}
}
