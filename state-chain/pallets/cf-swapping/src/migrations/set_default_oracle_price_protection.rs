use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::*;

pub struct Migration<T>(PhantomData<T>);

// This migration sets default oracle price slippage protection all pools that support oracle price.
const STABLE_COIN_SLIPPAGE: BasisPoints = 25;
const NON_STABLE_SLIPPAGE: BasisPoints = 50;

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let set_default_protection = |base_asset: Asset, bps: BasisPoints| {
			if let Some(asset_pair) = AssetPair::new(base_asset, STABLE_ASSET) {
				DefaultOraclePriceSlippageProtection::<T>::insert(asset_pair, bps);
				log::info!(
					"Set default oracle slippage protection for {asset_pair:?} to {bps} bps"
				);
			} else {
				log::error!("Failed to set default oracle slippage protection: Pool does not exist for assets: {:?}/{:?}", base_asset, STABLE_ASSET);
			}
		};

		Asset::all().for_each(|asset| match asset {
			Asset::Usdt |
			Asset::ArbUsdc |
			Asset::ArbUsdt |
			Asset::HubUsdc |
			Asset::HubUsdt |
			Asset::SolUsdc |
			Asset::SolUsdt => {
				set_default_protection(asset, STABLE_COIN_SLIPPAGE);
			},
			Asset::ArbEth | Asset::Eth | Asset::Btc | Asset::Sol | Asset::Wbtc => {
				set_default_protection(asset, NON_STABLE_SLIPPAGE);
			},
			Asset::Flip | Asset::HubDot | Asset::Dot => {
				// No oracle price for these assets
			},
			Asset::Usdc => {
				// Usdc cannot be set because it is the quote asset for all pools
			},
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(
			DefaultOraclePriceSlippageProtection::<T>::get(
				AssetPair::new(Asset::Eth, STABLE_ASSET)
					.ok_or(DispatchError::from("Pool should exist"))?
			),
			NON_STABLE_SLIPPAGE
		);
		assert_eq!(
			DefaultOraclePriceSlippageProtection::<T>::get(
				AssetPair::new(Asset::Usdt, STABLE_ASSET)
					.ok_or(DispatchError::from("Pool should exist"))?
			),
			STABLE_COIN_SLIPPAGE
		);

		log::info!("Post-upgrade checks for SetDefaultOracleSlippageProtection passed");

		Ok(())
	}
}
