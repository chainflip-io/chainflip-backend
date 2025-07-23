//! Chainlink specific types

use cf_amm_math::Price;

use crate::electoral_systems::{
	oracle_price::{price::*, state_machine::*},
	state_machine::common_imports::*,
};

def_derive! {
	#[derive(TypeInfo, Sequence, PartialOrd, Ord, Copy)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum ChainlinkAssetpair {
		BtcUsd,
		EthUsd,
		SolUsd,
		UsdcUsd,
		UsdtUsd
	}
}

impl AssetPairTrait for ChainlinkAssetpair {
	fn to_price_unit(&self) -> PriceUnit {
		use ChainlinkAssetpair::*;
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

pub type ChainlinkPrice = Fraction<99_999_999>;

pub fn get_all_latest_prices_with_statechain_encoding<T>(
	state: &OraclePriceTracker<T>,
) -> BTreeMap<PriceAsset, (Price, PriceStaleness)>
where
	T: OPTypes<AssetPair = ChainlinkAssetpair, Price = ChainlinkPrice>,
{
	all::<ChainlinkAssetpair>()
		.filter_map(|assetpair| {
			get_latest_price_with_statechain_encoding(state, assetpair)
				.map(|result| (assetpair.to_price_unit().base_asset, result))
		})
		.collect()
}

pub fn get_latest_price_with_statechain_encoding<T>(
	state: &OraclePriceTracker<T>,
	chainlink_assetpair: ChainlinkAssetpair,
) -> Option<(Price, PriceStaleness)>
where
	T: OPTypes<AssetPair = ChainlinkAssetpair, Price = ChainlinkPrice>,
{
	state
		.chain_states
		.get_latest_price(chainlink_assetpair)
		.and_then(|(price, status)| {
			let price_unit = chainlink_assetpair.to_price_unit();
			Some((price_with_unit_to_statechain_price(price, price_unit)?.into(), status))
		})
}
