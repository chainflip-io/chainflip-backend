// TODO: See if we can dedup this once the vault address stuff is deduped
use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError},
	eth::deposit_address::get_create_2_address,
	Arbitrum, Chain,
};
use cf_primitives::{chains::assets::arb, ChannelId};

use crate::{Environment, EvmEnvironment};
use cf_chains::evm::api::EvmEnvironmentProvider;

use super::AddressDerivation;

impl AddressDerivationApi<Arbitrum> for AddressDerivation {
	fn generate_address(
		source_asset: arb::Asset,
		channel_id: ChannelId,
	) -> Result<<Arbitrum as Chain>::ChainAccount, AddressDerivationError> {
		Ok(get_create_2_address(
			Environment::arb_vault_address(),
			<EvmEnvironment as EvmEnvironmentProvider<Arbitrum>>::token_address(source_asset),
			channel_id,
		))
	}

	fn generate_address_and_state(
		source_asset: <Arbitrum as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Arbitrum as Chain>::ChainAccount, <Arbitrum as Chain>::DepositChannelState),
		AddressDerivationError,
	> {
		Ok((
			<Self as AddressDerivationApi<Arbitrum>>::generate_address(source_asset, channel_id)?,
			Default::default(),
		))
	}
}
