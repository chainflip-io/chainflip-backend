use cf_chains::{address::AddressDerivationApi, Solana};

use super::AddressDerivation;

impl AddressDerivationApi<Solana> for AddressDerivation {
	fn generate_address(
		_source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		_channel_id: cf_primitives::ChannelId,
	) -> Result<
		<Solana as cf_chains::Chain>::ChainAccount,
		cf_chains::address::AddressDerivationError,
	> {
		unimplemented!()
	}

	fn generate_address_and_state(
		_source_asset: <Solana as cf_chains::Chain>::ChainAsset,
		_channel_id: cf_primitives::ChannelId,
	) -> Result<
		(
			<Solana as cf_chains::Chain>::ChainAccount,
			<Solana as cf_chains::Chain>::DepositChannelState,
		),
		cf_chains::address::AddressDerivationError,
	> {
		unimplemented!()
	}
}
