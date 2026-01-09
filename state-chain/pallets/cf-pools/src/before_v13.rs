use super::*;

/// This is a copy of PoolInfo except that we have preserved `limit_order_fee_hundredth_pips` and
/// `limit_order_total_fees_earned` for RPC compatibility reasons.
#[derive(Serialize, Deserialize, Clone, Copy, Encode, Decode)]
pub struct PoolInfo {
	/// The fee taken, when limit orders are used, from swap inputs that contributes to liquidity
	/// provider earnings
	pub limit_order_fee_hundredth_pips: u32,
	/// The fee taken, when range orders are used, from swap inputs that contributes to liquidity
	/// provider earnings
	pub range_order_fee_hundredth_pips: u32,
	/// The total fees earned in this pool by range orders.
	pub range_order_total_fees_earned: PoolPairsMap<cf_amm::math::Amount>,
	/// The total fees earned in this pool by limit orders.
	pub limit_order_total_fees_earned: PoolPairsMap<cf_amm::math::Amount>,
	/// The total amount of assets that have been bought by range orders in this pool.
	pub range_total_swap_inputs: PoolPairsMap<cf_amm::math::Amount>,
	/// The total amount of assets that have been bought by limit orders in this pool.
	pub limit_total_swap_inputs: PoolPairsMap<cf_amm::math::Amount>,
}

impl From<super::PoolInfo> for PoolInfo {
	fn from(value: super::PoolInfo) -> Self {
		PoolInfo {
			limit_order_fee_hundredth_pips: 0,
			range_order_fee_hundredth_pips: value.range_order_fee_hundredth_pips,
			limit_order_total_fees_earned: Default::default(),
			range_order_total_fees_earned: value.range_order_total_fees_earned,
			range_total_swap_inputs: value.range_total_swap_inputs,
			limit_total_swap_inputs: value.limit_total_swap_inputs,
		}
	}
}

// impl From<PoolInfo> for super::PoolInfo {
// 	fn from(value: PoolInfo) -> Self {
// 		super::PoolInfo {
// 			range_order_fee_hundredth_pips: value.range_order_fee_hundredth_pips,
// 			range_order_total_fees_earned: value.range_order_total_fees_earned,
// 			range_total_swap_inputs: value.range_total_swap_inputs,
// 			limit_total_swap_inputs: value.limit_total_swap_inputs,
// 		}
// 	}
// }
