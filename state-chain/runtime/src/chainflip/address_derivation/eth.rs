use crate::{Environment, EthEnvironment};
use cf_chains::{
	address::AddressDerivationApi,
	eth::{api::EthEnvironmentProvider, deposit_address::get_create_2_address},
	Chain, Ethereum,
};
use cf_primitives::{chains::assets::eth, ChannelId};
use sp_runtime::DispatchError;

use super::AddressDerivation;

impl AddressDerivationApi<Ethereum> for AddressDerivation {
	fn generate_address(
		source_asset: eth::Asset,
		channel_id: ChannelId,
	) -> Result<<Ethereum as Chain>::ChainAccount, DispatchError> {
		Ok(get_create_2_address(
			Environment::eth_vault_address(),
			EthEnvironment::token_address(source_asset),
			channel_id,
		))
	}
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use cf_chains::Ethereum;
	use cf_primitives::chains::assets::eth::Asset;
	use pallet_cf_environment::EthereumSupportedAssets;

	frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
		// Expect address generation to be successfully for native ETH
		assert!(<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			eth::Asset::Eth,
			1
		)
		.is_ok());
		// The genesis build is not running, so we have to add it manually
		EthereumSupportedAssets::<Runtime>::insert(Asset::Flip, sp_core::H160([1; 20]));
		// Expect address generation to be successfully for ERC20 Flip token
		assert!(<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			eth::Asset::Flip,
			1
		)
		.is_ok());

		// Address derivation for Dot is currently unimplemented.
		// Expect address generation to return an error for unsupported assets. Because we are
		// running a test gainst ETH the DOT asset will be always unsupported.
		// assert!(AddressDerivation::generate_address(Asset::Dot, 1).is_err());
	});
}
