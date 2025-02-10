use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError},
	sol::{
		api::SolanaEnvironment,
		sol_tx_core::{address_derivation::derive_deposit_address, PdaAndBump},
	},
	Solana,
};

use super::AddressDerivation;
use crate::SolEnvironment;

impl AddressDerivationApi<Solana> for AddressDerivation {
	fn generate_address(
		source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		channel_id: cf_primitives::ChannelId,
	) -> Result<<Solana as cf_chains::Chain>::ChainAccount, AddressDerivationError> {
		<Self as AddressDerivationApi<Solana>>::generate_address_and_state(source_asset, channel_id)
			.map(|(address, _state)| address)
	}

	fn generate_address_and_state(
		_source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		channel_id: cf_primitives::ChannelId,
	) -> Result<
		(
			<Solana as cf_chains::Chain>::ChainAccount,
			<Solana as cf_chains::Chain>::DepositChannelState,
		),
		AddressDerivationError,
	> {
		let api_env = SolEnvironment::api_environment()
			.map_err(|_| AddressDerivationError::MissingSolanaApiEnvironment)?;

		derive_deposit_address(channel_id, api_env.vault_program)
			.map(|PdaAndBump { address, bump }| (address, bump))
			.map_err(AddressDerivationError::SolanaDerivationError)
	}
}
