//! Chainlink specific types

use cf_amm_math::Price;

use crate::{
	electoral_systems::{
		oracle_price::{price::*, state_machine::*},
		state_machine::{common_imports::*, core::*},
	},
	generic_tools::*,
};

def_derive! {
	#[derive(TypeInfo, Sequence, PartialOrd, Ord)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum ChainlinkAssetPair {
		BtcUsd,
		EthUsd,
		SolUsd,
		UsdcUsd,
		UsdtUsd
	}
}

impl AssetPairTrait for ChainlinkAssetPair {
	fn to_price_unit(&self) -> PriceUnit {
		use ChainlinkAssetPair::*;
		use PriceAsset::*;
		let base_asset = match self {
			BtcUsd => Btc,
			EthUsd => Eth,
			SolUsd => Sol,
			UsdcUsd => Usdc,
			UsdtUsd => Usdt,
		};
		PriceUnit { base_asset, quote_asset: PriceAsset::Usd }
	}
}

pub type ChainlinkPrice = FractionImpl<99_999_999>;

pub fn get_current_chainlink_prices<T>(
	state: &OraclePriceTracker<T>,
) -> BTreeMap<PriceAsset, (Price, PriceStaleness)>
where
	T: OPTypes<AssetPair = ChainlinkAssetPair, Price = ChainlinkPrice>,
{
	//
	// WARNING: We are currently assuming that USD == USDC!
	//
	state
		.chain_states
		.get_latest_prices()
		.into_iter()
		.map(|(chainlink_assetpair, (price, status))| {
			let price_unit = chainlink_assetpair.to_price_unit();
			(
				price_unit.base_asset,
				(price_with_unit_to_statechain_price(price, price_unit).into(), status),
			)
		})
		.collect()
}
