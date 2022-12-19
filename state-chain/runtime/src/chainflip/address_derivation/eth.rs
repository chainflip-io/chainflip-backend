use crate::{Environment, EthEnvironment};
use cf_chains::{eth::ingress_address::get_create_2_address, Chain, ChainEnvironment, Ethereum};
use cf_primitives::{chains::assets::eth, IntentId};
use cf_traits::AddressDerivationApi;
use sp_runtime::DispatchError;

use super::AddressDerivation;

impl AddressDerivationApi<Ethereum> for AddressDerivation {
	fn generate_address(
		ingress_asset: eth::Asset,
		intent_id: IntentId,
	) -> Result<<Ethereum as Chain>::ChainAccount, DispatchError> {
		Ok(get_create_2_address(
			ingress_asset,
			Environment::eth_vault_address(),
			match ingress_asset {
				eth::Asset::Eth => None,
				_ => Some(
					EthEnvironment::lookup(ingress_asset)
						.expect("ERC20 asset to be supported!")
						.to_fixed_bytes()
						.to_vec(),
				),
			},
			intent_id,
		)
		.into())
	}
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use cf_chains::Ethereum;
	use cf_primitives::Asset;
	use pallet_cf_environment::EthereumSupportedAssets;

	frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
		// Expect address generation to be successfully for native ETH
		assert!(<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			eth::Asset::Eth,
			1
		)
		.is_ok());
		// The genesis build is not running, so we have to add it manually
		EthereumSupportedAssets::<Runtime>::insert(Asset::Flip, [0; 20]);
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
