use super::{MockPallet, MockPalletStorage};
use crate::FlipBurnInfo;
use cf_primitives::AssetAmount;

pub struct MockFlipBurnInfo;

impl MockPallet for MockFlipBurnInfo {
	const PREFIX: &'static [u8] = b"MockFlipBurnInfo";
}

const FLIP_TO_BURN: &[u8] = b"FLIP_TO_BURN";

impl MockFlipBurnInfo {
	pub fn set_flip_to_burn(flip_to_burn: AssetAmount) {
		Self::put_value(FLIP_TO_BURN, flip_to_burn);
	}

	pub fn peek_flip_to_burn() -> AssetAmount {
		Self::get_value(FLIP_TO_BURN).unwrap_or_default()
	}
}

impl FlipBurnInfo for MockFlipBurnInfo {
	fn take_flip_to_burn() -> AssetAmount {
		Self::take_value(FLIP_TO_BURN).unwrap_or_default()
	}
}
