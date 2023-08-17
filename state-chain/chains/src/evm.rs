use crate::{eth::Address as EvmAddress, *};

use self::api::EvmReplayProtection;

pub mod api;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum EthereumContract {
	StateChainGateway,
	KeyManager,
	Vault,
}

pub type EthereumChainId = u64;

/// Provides the environment data for ethereum-like chains.
pub trait EvmEnvironmentProvider<C: Chain> {
	fn token_address(asset: <C as Chain>::ChainAsset) -> Option<EvmAddress>;
	fn contract_address(contract: EthereumContract) -> EvmAddress;
	fn chain_id() -> EthereumChainId;
	fn next_nonce() -> u64;

	fn replay_protection(contract: EthereumContract) -> EvmReplayProtection {
		EvmReplayProtection {
			nonce: Self::next_nonce(),
			chain_id: Self::chain_id(),
			key_manager_address: Self::contract_address(EthereumContract::KeyManager),
			contract_address: Self::contract_address(contract),
		}
	}
}
