use crate::{
	mocks::{MockPallet, MockPalletStorage},
	LiabilityTracker,
};
use cf_chains::ForeignChainAddress;
use cf_primitives::{accounting::AssetBalance, Asset, AssetAmount};
use frame_support::sp_runtime::Saturating;
use sp_std::collections::btree_map::BTreeMap;

pub struct MockLiabilityTracker;

impl MockPallet for MockLiabilityTracker {
	const PREFIX: &'static [u8] = b"MockLiabilityTracker";
}

impl MockLiabilityTracker {
	pub fn total_liabilities(asset: Asset) -> AssetAmount {
		Self::get_storage::<Asset, BTreeMap<ForeignChainAddress, AssetAmount>>(LIABILITIES, asset)
			.unwrap_or_default()
			.values()
			.sum::<AssetAmount>()
	}
}

const LIABILITIES: &[u8] = b"LIABILITIES";

impl LiabilityTracker for MockLiabilityTracker {
	fn record_liability(account_id: ForeignChainAddress, asset_balance: AssetBalance) {
		Self::mutate_storage::<Asset, _, BTreeMap<ForeignChainAddress, AssetAmount>, _, _>(
			LIABILITIES,
			&asset_balance.asset(),
			|value: &mut Option<BTreeMap<ForeignChainAddress, AssetAmount>>| {
				value
					.get_or_insert_default()
					.entry(account_id)
					.or_default()
					.saturating_accrue(asset_balance.amount());
			},
		);
	}
}
