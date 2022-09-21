use crate::Environment;
use cf_chains::eth::ingress_address::get_create_2_address;
use cf_primitives::Asset;
use cf_traits::AddressDerivationApi;
use frame_support::ensure;
use sp_runtime::DispatchError;

pub struct AddressDerivation;

impl AddressDerivationApi for AddressDerivation {
	fn generate_address(
		ingress_asset: cf_primitives::ForeignChainAsset,
		intent_id: cf_primitives::IntentId,
	) -> Result<cf_primitives::ForeignChainAddress, DispatchError> {
		match ingress_asset.chain {
			cf_primitives::ForeignChain::Ethereum => {
				ensure!(
					ingress_asset.asset == Asset::Eth ||
						Environment::supported_eth_assets(ingress_asset.asset).is_some(),
					DispatchError::Other(
						"Address derivation is currently unsupported for this asset!",
					)
				);
				Ok(cf_primitives::ForeignChainAddress::Eth(get_create_2_address(
					ingress_asset.asset,
					Environment::eth_vault_address(),
					if ingress_asset.asset == Asset::Eth {
						None
					} else {
						Some(
							Environment::supported_eth_assets(ingress_asset.asset)
								.expect("ERC20 asset to be supported!")
								.to_vec(),
						)
					},
					intent_id,
				)))
			},
			cf_primitives::ForeignChain::Polkadot => todo!(),
		}
	}
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use pallet_cf_environment::SupportedEthAssets;

	frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
		// Expect address generation to be successfully for native ETH
		assert!(AddressDerivation::generate_address(
			cf_primitives::ForeignChainAsset {
				chain: cf_primitives::ForeignChain::Ethereum,
				asset: cf_primitives::Asset::Eth,
			},
			1
		)
		.is_ok());
		// Expect address generation to return an error for unsupported assets. Because we are
		// running a test gainst ETH the DOT asset will be always unsupported.
		assert!(AddressDerivation::generate_address(
			cf_primitives::ForeignChainAsset {
				chain: cf_primitives::ForeignChain::Ethereum,
				asset: cf_primitives::Asset::Dot,
			},
			1
		)
		.is_err());
		// The genesis build is not running, so we have to add it manually
		SupportedEthAssets::<Runtime>::insert(Asset::Flip, [0; 20]);
		// Expect address generation to be successfully for ERC20 Flip token
		assert!(AddressDerivation::generate_address(
			cf_primitives::ForeignChainAsset {
				chain: cf_primitives::ForeignChain::Ethereum,
				asset: cf_primitives::Asset::Flip,
			},
			1
		)
		.is_ok());
	});
}
