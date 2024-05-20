use super::{MockPallet, MockPalletStorage};
use crate::{SwapQueueApi, SwapType};
use cf_chains::{address::EncodedAddress, SwapOrigin};
use cf_primitives::{Asset, AssetAmount, SwapId};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockSwap {
	pub from: Asset,
	pub to: Asset,
	pub amount: AssetAmount,
	pub swap_type: SwapType,
}

pub struct MockSwapQueueApi;

impl MockPallet for MockSwapQueueApi {
	const PREFIX: &'static [u8] = b"MockSwapQueueApi";
}

const SWAP_QUEUE: &[u8] = b"SWAP_QUEUE";

impl MockSwapQueueApi {
	pub fn get_swap_queue() -> Vec<MockSwap> {
		Self::get_value(SWAP_QUEUE).unwrap_or_default()
	}
}

impl SwapQueueApi for MockSwapQueueApi {
	type BlockNumber = u128;

	fn schedule_swap(
		from: Asset,
		to: Asset,
		amount: AssetAmount,
		swap_type: SwapType,
		_swap_origin: SwapOrigin,
		_destination_address: Option<EncodedAddress>,
		_broker_fee: Option<AssetAmount>,
	) -> SwapId {
		Self::mutate_value(SWAP_QUEUE, |queue: &mut Option<Vec<MockSwap>>| {
			queue.get_or_insert(vec![]).push(MockSwap { from, to, amount, swap_type });
		});
		Self::get_value::<Vec<MockSwap>>(SWAP_QUEUE).unwrap().len() as u64
	}
}
