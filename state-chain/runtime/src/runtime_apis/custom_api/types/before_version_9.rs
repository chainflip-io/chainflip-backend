use super::*;

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Default)]
pub struct LiquidityProviderInfo {
	pub refund_addresses: Vec<(ForeignChain, Option<ForeignChainAddress>)>,
	pub balances: Vec<(Asset, AssetAmount)>,
	pub earned_fees: before_version_16::AssetMap<AssetAmount>,
	pub boost_balances: before_version_16::AssetMap<Vec<LiquidityProviderBoostPoolInfo>>,
}

impl From<LiquidityProviderInfo> for super::LiquidityProviderInfo {
	fn from(old: LiquidityProviderInfo) -> Self {
		before_version_16::LiquidityProviderInfo {
			refund_addresses: old.refund_addresses,
			balances: old.balances,
			earned_fees: old.earned_fees,
			boost_balances: old.boost_balances,
			..Default::default()
		}
		.into()
	}
}
