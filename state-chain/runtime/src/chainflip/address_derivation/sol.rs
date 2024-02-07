use cf_chains::{address::AddressDerivationApi, assets::sol::Asset, Solana};

use super::AddressDerivation;

impl AddressDerivationApi<Solana> for AddressDerivation {
	fn generate_address(
		source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		_channel_id: cf_primitives::ChannelId,
	) -> Result<
		<Solana as cf_chains::Chain>::ChainAccount,
		cf_chains::address::AddressDerivationError,
	> {
		match source_asset {
			Asset::Sol => todo!("Derive using cf-environment::SolanaVaultAddress"),
		}
	}

	fn generate_address_and_state(
		source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		_channel_id: cf_primitives::ChannelId,
	) -> Result<
		(
			<Solana as cf_chains::Chain>::ChainAccount,
			<Solana as cf_chains::Chain>::DepositChannelState,
		),
		cf_chains::address::AddressDerivationError,
	> {
		match source_asset {
			Asset::Sol => todo!("Derive Derive using cf-environment::SolanaVaultAddress"),
		}
	}
}
