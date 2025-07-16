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

impl ChainlinkAssetPair {
	pub fn base_asset(&self) -> PriceAsset {
		match self {
			ChainlinkAssetPair::BtcUsd => PriceAsset::Btc,
			ChainlinkAssetPair::EthUsd => PriceAsset::Eth,
		}
	}
}

pub struct PriceUnit {
	pub base_asset: PriceAsset,
	pub quote_asset: PriceAsset,
}

pub struct Base10Exponent(i16);

impl Base10Exponent {
	pub fn inv(&self) -> Self {
		Base10Exponent(-self.0)
	}
}

impl PriceUnit {
	pub fn get_exponent(&self) -> Base10Exponent {
		Base10Exponent(self.base_asset.decimals() as i16 - self.quote_asset.decimals() as i16)
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

pub type U256Base = [u64; 4];
type Denom = u128;

pub fn denom(d: Denom) -> U256 {
	U256::from(d) + 1
}

def_derive! {
	/// Note that this can handle denominators of up to u128::MAX + 1
	#[derive(Default, PartialOrd, Ord, TypeInfo)]
	pub struct FractPrice<const U: Denom>(U256);
}

impl<const U: Denom> FractPrice<U> {
	pub fn convert<const V: Denom>(self) -> FractPrice<V> {
		FractPrice(mul_div_floor(self.0, denom(V), denom(U)))
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

type StatechainPrice = FractPrice<{ u128::MAX }>;

/// WARNING, this assumes PRICE_FRACTIONAL_BITS = 128!
impl Into<Price> for FractPrice<{ u128::MAX }> {
	fn into(self) -> Price {
		debug_assert_eq!(PRICE_FRACTIONAL_BITS, 128);
		self.0
	}
}

pub fn price_with_unit_to_statechain_price<const U: u128>(
	price: FractPrice<U>,
	unit: PriceUnit,
) -> StatechainPrice {
	price
		.apply_exponent(Base10Exponent(
			unit.quote_asset.decimals() as i16 - unit.base_asset.decimals() as i16,
		))
		.convert()
}
