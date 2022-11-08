use crate::BidInfo;

pub struct MockBidInfo;

impl BidInfo for MockBidInfo {
	type Balance = u128;
	fn get_min_backup_bid() -> Self::Balance {
		todo!()
	}
}
