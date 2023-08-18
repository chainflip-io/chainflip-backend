use crate::{eth::Address as EvmAddress, *};

use self::api::EvmReplayProtection;

pub mod api;

pub type EthereumChainId = u64;

/// Provides the environment data for ethereum-like chains.
pub trait EvmEnvironmentProvider<C: Chain> {
	type Contract;

	fn token_address(asset: <C as Chain>::ChainAsset) -> Option<EvmAddress>;
	fn contract_address(contract: Self::Contract) -> EvmAddress;
	fn key_manager_address() -> EvmAddress;
	fn chain_id() -> EthereumChainId;
	fn next_nonce() -> u64;

	fn replay_protection(contract: Self::Contract) -> EvmReplayProtection {
		EvmReplayProtection {
			nonce: Self::next_nonce(),
			chain_id: Self::chain_id(),
			key_manager_address: Self::key_manager_address(),
			contract_address: Self::contract_address(contract),
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SchnorrVerificationComponents {
	/// Scalar component
	pub s: [u8; 32],
	/// The challenge, expressed as a truncated keccak hash of a pair of coordinates.
	pub k_times_g_address: [u8; 20],
}
