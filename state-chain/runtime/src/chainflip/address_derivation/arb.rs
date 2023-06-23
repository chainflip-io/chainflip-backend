use cf_chains::{address::AddressDerivationApi, Arbitrum, Chain};
use cf_primitives::{chains::assets::arb, ChannelId};
use sp_runtime::DispatchError;

use super::AddressDerivation;

impl AddressDerivationApi<Arbitrum> for AddressDerivation {
	fn generate_address(
		_source_asset: arb::Asset,
		_channel_id: ChannelId,
	) -> Result<<Arbitrum as Chain>::ChainAccount, DispatchError> {
		todo!()
	}
}
