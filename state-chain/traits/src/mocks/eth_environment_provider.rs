use cf_chains::{
	evm::{api::EvmEnvironmentProvider, Address},
	Chain,
};

/// A mock that just returns defaults for the KeyManager and Chain ID.
pub struct MockEvmEnvironment;

impl<C: Chain> EvmEnvironmentProvider<C> for MockEvmEnvironment {
	fn key_manager_address() -> Address {
		Default::default()
	}

	fn vault_address() -> Address {
		Default::default()
	}

	fn next_nonce() -> u64 {
		Default::default()
	}

	fn token_address(_asset: <C as Chain>::ChainAsset) -> Option<Address> {
		Some(Default::default())
	}

	fn chain_id() -> cf_chains::evm::api::EvmChainId {
		Default::default()
	}
}
