use cf_amm_math::{mul_div_floor, PRICE_FRACTIONAL_BITS};
use cf_primitives::{Asset, Price};
use scale_info::prelude::marker::ConstParamTy;
use sp_core::U256;

use crate::electoral_systems::state_machine::common_imports::*;

#[cfg(test)]
use proptest_derive::Arbitrary;


def_derive! {
	#[derive(TypeInfo, Sequence, PartialOrd, Ord)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum ChainlinkAssetPair {
		BtcUsd,
		EthUsd
	}
}

def_derive! {
	pub enum BaseUnit {
		FineBtc,
		FineEth,
	}
}

pub struct PriceUnit {
	quote: PriceAsset,
	base: PriceAsset,
}

pub struct Base10Exponent(i16);

impl Base10Exponent {
	pub fn inv(&self) -> Self {
		Base10Exponent(-self.0)
	}
}

impl PriceUnit {
	pub fn get_exponent(&self) -> Base10Exponent {
		Base10Exponent(self.quote.decimals() as i16 - self.base.decimals() as i16)
	}
}

#[derive(ConstParamTy, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub enum PriceAsset {
	Btc,
	Eth,
	Usdc,
}

impl PriceAsset {
	pub fn decimals(&self) -> u8 {
		match self {
			PriceAsset::Btc => 8,
			PriceAsset::Eth => 12,
			PriceAsset::Usdc => 6,
		}
	}
}

pub struct FractPrice<const U: u128>(U256);

pub fn convert<const U: u128, const V: u128>(price: FractPrice<U>) -> FractPrice<V> {
	FractPrice(mul_div_floor(price.0, V.into(), U))
}

impl<const U: u128> FractPrice<U> {
	pub fn convert<const V: u128>(self) -> FractPrice<V> {
		FractPrice(mul_div_floor(self.0, V.into(), U))
	}

	pub fn apply_exponent(&self, exp: Base10Exponent) -> Self {
		let exp = exp.0;
		if exp < 0 {
			FractPrice(mul_div_floor(self.0, 1.into(), 10u128.pow((exp * -1) as u32)))
		} else {
			FractPrice(mul_div_floor(self.0, 10u128.pow(exp as u32).into(), 1))
		}
	}
}

type SCPrice = FractPrice<{ PRICE_FRACTIONAL_BITS as u128 }>;

pub fn convert_asset_prices<const U: u128>(
	prices: BTreeMap<PriceAsset, FractPrice<U>>,
) -> BTreeMap<PriceAsset, SCPrice> {
	prices
		.into_iter()
		.map(|(asset, price)| {
			let e = PriceUnit { quote: asset, base: PriceAsset::Usdc }.get_exponent();
			(asset, price.apply_exponent(e.inv()).convert())
		})
		.collect()
}

