pub mod mock_btc;
pub mod mock_eth;

pub struct MockAffiliateRegistry {}

impl cf_traits::AffiliateRegistry for MockAffiliateRegistry {
	type AccountId = u64;

	fn lookup(
		_broker_id: &Self::AccountId,
		_affiliate_idx: cf_primitives::AffiliateShortId,
	) -> Option<Self::AccountId> {
		None
	}
}
