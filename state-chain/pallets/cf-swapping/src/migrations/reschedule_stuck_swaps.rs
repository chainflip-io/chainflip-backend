use core::marker::PhantomData;

use cf_primitives::{Asset, AssetAmount, SwapId, SwapRequestId, SWAP_DELAY_BLOCKS};
use frame_support::{
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_runtime::traits::BlockNumberProvider;

use crate::{
	Config, Event, ScheduledSwaps, Swap, SwapFailureReason, SwapRequestState, SwapRequests,
};

pub struct RescheduleStuckSwaps<T>(PhantomData<T>);

use sp_std::{vec, vec::Vec};

impl<T: Config> UncheckedOnRuntimeUpgrade for RescheduleStuckSwaps<T> {
	fn on_runtime_upgrade() -> Weight {
		fn swap<T: Config>(
			swap_request_id: SwapRequestId,
			swap_id: SwapId,
			from: Asset,
			to: Asset,
			input_amount: AssetAmount,
		) -> Swap<T> {
			let execute_at =
				frame_system::Pallet::<T>::current_block_number() + SWAP_DELAY_BLOCKS.into();

			Swap {
				swap_id,
				swap_request_id,
				from,
				to,
				input_amount,
				refund_params: None,
				execute_at,
			}
		}

		let swaps_to_reschedule: Vec<Swap<T>> = vec![
			swap(SwapRequestId(896597), SwapId(1210204), Asset::Usdt, Asset::Eth, 1_334_836), /* ~$1 */
			swap(SwapRequestId(1079456), SwapId(1209168), Asset::ArbUsdc, Asset::ArbEth, 189_370), /* ~$0.2 */
		];

		let mut rescheduled_swaps_count = 0;

		ScheduledSwaps::<T>::mutate(|scheduled_swaps| {
			for swap in &swaps_to_reschedule {
				// Sanity check: only reschedule if swap request does exist and assets match
				if let Some(swap_request) = SwapRequests::<T>::get(swap.swap_request_id) {
					if swap_request.input_asset == swap.from &&
						swap_request.output_asset == swap.to &&
						// Only fee swaps are expected:
						(swap_request.state == SwapRequestState::IngressEgressFee ||
							swap_request.state == SwapRequestState::NetworkFee)
					{
						crate::Pallet::<T>::deposit_event(Event::<T>::SwapRescheduled {
							swap_id: swap.swap_id,
							execute_at: swap.execute_at,
							reason: SwapFailureReason::PriceImpactLimit,
						});

						log::info!("Rescheduled swap: {}", swap.swap_request_id);

						scheduled_swaps.insert(swap.swap_id, swap.clone());

						rescheduled_swaps_count += 1;
					} else {
						log::error!(
							"Parameters don't match for swap request: {}",
							swap.swap_request_id
						);
					}
				} else {
					log::warn!("Swap request does not exist: {}", swap.swap_request_id);
				}
			}
		});

		T::DbWeight::get()
			.reads_writes(1 + swaps_to_reschedule.len() as u64, 1 + rescheduled_swaps_count as u64)
	}
}
