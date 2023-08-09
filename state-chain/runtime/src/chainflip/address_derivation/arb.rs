use cf_chains::{
	address::AddressDerivationApi,
	eth::{api::EthEnvironmentProvider, deposit_address::get_create_2_address},
	Arbitrum, Chain,
};
use cf_primitives::{chains::assets::arb, ChannelId};
use sp_runtime::DispatchError;

use crate::{ArbEnvironment, Environment};

use super::AddressDerivation;

impl AddressDerivationApi<Arbitrum> for AddressDerivation {
	fn generate_address(
		source_asset: arb::Asset,
		channel_id: ChannelId,
	) -> Result<<Arbitrum as Chain>::ChainAccount, DispatchError> {
		Ok(get_create_2_address(
			Environment::arb_vault_address(),
			ArbEnvironment::token_address(source_asset),
			channel_id,
		))
	}
}
