use super::{MockPallet, MockPalletStorage};
use crate::{SwapQueueApi, SwapType};
use cf_primitives::{Asset, AssetAmount};
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
	) -> (u64, Self::BlockNumber) {
		Self::mutate_value(SWAP_QUEUE, |queue: &mut Option<Vec<MockSwap>>| {
			match queue {
				Some(queue) => queue.push(MockSwap { from, to, amount, swap_type }),
				None => *queue = Some(vec![MockSwap { from, to, amount, swap_type }]),
			};
		});
		(Self::get_value::<Vec<MockSwap>>(SWAP_QUEUE).unwrap().len() as u64, 0)
	}
}
