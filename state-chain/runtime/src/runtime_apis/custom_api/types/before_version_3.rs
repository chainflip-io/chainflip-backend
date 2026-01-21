pub use super::*;

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct BrokerInfo {
	pub earned_fees: Vec<(Asset, AssetAmount)>,
}
