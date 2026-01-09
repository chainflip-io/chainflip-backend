use super::*;

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcLendingPool<Amount> {
	pub asset: Asset,
	/// Total amount collectively owed to lenders
	pub total_amount: Amount,
	/// The amount available for borrowing. Could be larger than `total_amount` in a rare edge case
	/// where `total_owed_to_network` is not 0 despite all loans having been fully repaid (in
	/// which case `available_amount` == `total_amount` + `total_owed_to_network`).
	pub available_amount: Amount,
	pub utilisation_rate: Permill,
	pub current_interest_rate: Permill,
	#[serde(flatten)]
	pub config: LendingPoolConfiguration,
}

impl<Amount: Default> From<RpcLendingPool<Amount>> for super::RpcLendingPool<Amount> {
	fn from(value: RpcLendingPool<Amount>) -> Self {
		Self {
			asset: value.asset,
			total_amount: value.total_amount,
			available_amount: value.available_amount,
			utilisation_rate: value.utilisation_rate,
			owed_to_network: Default::default(),
			current_interest_rate: value.current_interest_rate,
			config: value.config,
		}
	}
}
