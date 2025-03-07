use cf_primitives::{Asset, AssetAmount};

use crate::{TradingStrategyParameters, TradingStrategyParametersProvider};

use super::{MockPallet, MockPalletStorage};

pub struct MockTradingStrategyParameters {}

impl MockPallet for MockTradingStrategyParameters {
	const PREFIX: &'static [u8] = b"TradingStrategyParametersProvider";
}

const ORDER_UPDATE_THRESHOLDS: &[u8] = b"ORDER_UPDATE_THRESHOLDS";

impl MockTradingStrategyParameters {
	pub fn set_order_update_threshold(asset: &Asset, new_threshold: AssetAmount) {
		Self::mutate_value(ORDER_UPDATE_THRESHOLDS, |thresholds| {
			let thresholds: &mut TradingStrategyParameters = thresholds.get_or_insert_default();
			thresholds.order_update_thresholds.try_insert(*asset, new_threshold).unwrap();
		})
	}
}

impl TradingStrategyParametersProvider for MockTradingStrategyParameters {
	fn get_parameters() -> TradingStrategyParameters {
		Self::get_value::<TradingStrategyParameters>(ORDER_UPDATE_THRESHOLDS).unwrap_or_default()
	}
}
