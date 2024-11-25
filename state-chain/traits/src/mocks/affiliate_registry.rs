use cf_primitives::AffiliateShortId;
use frame_support::{parameter_types, sp_runtime::BoundedBTreeMap, traits::ConstU32};

use crate::AffiliateRegistry;

parameter_types! {
	pub storage AffiliateMapping: BoundedBTreeMap<(u64, AffiliateShortId), u64, ConstU32<100>> = Default::default();
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

impl AffiliateRegistry for MockAffiliateRegistry {
	type AccountId = u64;

	fn get_account_id(
		broker_id: &Self::AccountId,
		affiliate_short_id: AffiliateShortId,
	) -> Option<Self::AccountId> {
		AffiliateMapping::get().get(&(*broker_id, affiliate_short_id)).copied()
	}

	fn get_short_id(
		broker_id: &Self::AccountId,
		affiliate_id: &Self::AccountId,
	) -> Option<AffiliateShortId> {
		for ((broker, short_id), affiliate) in AffiliateMapping::get().iter() {
			if broker_id == broker && affiliate_id == affiliate {
				return Some(*short_id);
			}
		}
		None
	}
}
