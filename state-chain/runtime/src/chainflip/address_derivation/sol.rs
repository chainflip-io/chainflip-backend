use cf_chains::{
	address::AddressDerivationApi, sol::sol_tx_core::address_derivation::derive_deposit_channel,
	Solana,
};

use super::AddressDerivation;
use crate::Environment;

impl AddressDerivationApi<Solana> for AddressDerivation {
	fn generate_address(
		source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		channel_id: cf_primitives::ChannelId,
	) -> Result<
		<Solana as cf_chains::Chain>::ChainAccount,
		cf_chains::address::AddressDerivationError,
	> {
		<Self as AddressDerivationApi<Solana>>::generate_address_and_state(source_asset, channel_id)
			.map(|(address, _state)| address)
	}

	fn generate_address_and_state(
		source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		channel_id: cf_primitives::ChannelId,
	) -> Result<
		(
			<Solana as cf_chains::Chain>::ChainAccount,
			<Solana as cf_chains::Chain>::DepositChannelState,
		),
		cf_chains::address::AddressDerivationError,
	> {
		let vault_address = Environment::sol_vault_address();
		derive_deposit_channel(channel_id, source_asset, vault_address)
			.map(|deposit_channel| (deposit_channel.address, deposit_channel.state))
			.map_err(cf_chains::address::AddressDerivationError::SolanaDerivationError)
	}
}
