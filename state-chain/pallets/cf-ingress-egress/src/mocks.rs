use cf_primitives::AffiliateShortId;
use frame_support::parameter_types;
use sp_core::ConstU32;
use sp_runtime::BoundedBTreeMap;

pub mod mock_btc;
pub mod mock_eth;

parameter_types! {
	pub storage AffiliateMapping:  BoundedBTreeMap<(u64, AffiliateShortId), u64, ConstU32<100>> = Default::default();
}

pub struct MockAffiliateRegistry {}

impl MockAffiliateRegistry {
	pub fn register_affiliate(
		broker_id: u64,
		affiliate_id: u64,
		affiliate_short_id: AffiliateShortId,
	) {
		let mapping = AffiliateMapping::get()
			.try_mutate(|mapping| {
				mapping.insert((broker_id, affiliate_short_id), affiliate_id);
			})
			.unwrap();

		AffiliateMapping::set(&mapping);
	}
}

impl cf_traits::AffiliateRegistry for MockAffiliateRegistry {
	type AccountId = u64;

	fn lookup(
		broker_id: &Self::AccountId,
		affiliate_short_id: AffiliateShortId,
	) -> Option<Self::AccountId> {
		AffiliateMapping::get().get(&(*broker_id, affiliate_short_id)).copied()
	}
}
