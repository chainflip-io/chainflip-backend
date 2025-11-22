use crate::LpStatsApi;
use cf_primitives::{Asset, AssetAmount};
use sp_runtime::{traits::Zero, FixedU128, Saturating};

use super::{MockPallet, MockPalletStorage};

type AccountId = u64;

pub struct MockLpStatsApi;

impl MockPallet for MockLpStatsApi {
	const PREFIX: &'static [u8] = b"MockLpStatsApi";
}

const LP_DELTA_USD_VOLUME: &[u8] = b"LP_DELTA_USD_VOLUME";

impl LpStatsApi for MockLpStatsApi {
	type AccountId = AccountId;

	fn on_limit_order_filled(lp: &Self::AccountId, asset: &Asset, usd_amount: AssetAmount) {
		Self::mutate_storage::<(AccountId, Asset), _, _, _, _>(
			LP_DELTA_USD_VOLUME,
			&(lp, asset),
			|delta| {
				let delta = delta.get_or_insert(FixedU128::zero());

				*delta = delta.saturating_add(FixedU128::from_inner(usd_amount));
			},
		);
	}
}
