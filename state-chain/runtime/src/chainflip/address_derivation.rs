use crate::{Environment, EthEnvironment};
use cf_chains::{eth::ingress_address::get_create_2_address, ChainEnvironment};
use cf_primitives::{chains::assets::eth, Asset, ForeignChain, ForeignChainAddress, IntentId};
use cf_traits::AddressDerivationApi;
use sp_runtime::DispatchError;

pub struct AddressDerivation;

impl AddressDerivationApi for AddressDerivation {
	fn generate_address(
		ingress_asset: Asset,
		intent_id: IntentId,
	) -> Result<ForeignChainAddress, DispatchError> {
		match ingress_asset.into() {
			ForeignChain::Ethereum => {
				let eth_asset =
					eth::Asset::try_from(ingress_asset).expect("Checked for compatibilty.");
				Ok(ForeignChainAddress::Eth(get_create_2_address(
					eth_asset,
					Environment::eth_vault_address(),
					match eth_asset {
						eth::Asset::Eth => None,
						_ => Some(
							EthEnvironment::lookup(eth_asset)
								.expect("ERC20 asset to be supported!")
								.to_fixed_bytes()
								.to_vec(),
						),
					},
					intent_id,
				)))
			},
			ForeignChain::Polkadot => todo!(),
		}
	}
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use pallet_cf_environment::SupportedEthAssets;

	frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
		// Expect address generation to be successfully for native ETH
		assert!(AddressDerivation::generate_address(Asset::Eth, 1).is_ok());
		// The genesis build is not running, so we have to add it manually
		SupportedEthAssets::<Runtime>::insert(Asset::Flip, [0; 20]);
		// Expect address generation to be successfully for ERC20 Flip token
		assert!(AddressDerivation::generate_address(Asset::Flip, 1).is_ok());

		// Address derivation for Dot is currently unimplemented.
		// Expect address generation to return an error for unsupported assets. Because we are
		// running a test gainst ETH the DOT asset will be always unsupported.
		// assert!(AddressDerivation::generate_address(Asset::Dot, 1).is_err());
	});
}
