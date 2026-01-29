use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;
	use frame_support::Twox64Concat;

	impl NetworkFeeTracker {
		pub fn merge(&mut self, other: Self) {
			// Add the accumulated amounts together.
			self.accumulated_stable_amount =
				self.accumulated_stable_amount.saturating_add(other.accumulated_stable_amount);
			self.accumulated_fee = self.accumulated_fee.saturating_add(other.accumulated_fee);
			log::debug!(
				"Merged NetworkFeeTracker: accumulated_stable_amount={}, accumulated_fee={}",
				self.accumulated_stable_amount,
				self.accumulated_fee
			);

			// 2 swaps of the same request should always have the same network fee settings.
			if self.network_fee != other.network_fee {
				log::warn!("Merge two NetworkFeeTrackers with different FeeRateAndMinimum values.");
			}
		}
	}

	impl From<NetworkFeeTracker> for crate::NetworkFeeTracker {
		fn from(tracker: NetworkFeeTracker) -> crate::NetworkFeeTracker {
			crate::NetworkFeeTracker {
				network_fee: tracker.network_fee,
				accumulated_stable_amount: tracker.accumulated_stable_amount,
				accumulated_fee: tracker.accumulated_fee,
			}
		}
	}

	// No changes, but needs to support clone for the migration.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct NetworkFeeTracker {
		network_fee: FeeRateAndMinimum,
		accumulated_stable_amount: AssetAmount,
		accumulated_fee: AssetAmount,
	}

	// No changes, but needs to support clone for the migration.
	#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum FeeType<T: Config> {
		NetworkFee(NetworkFeeTracker),
		BrokerFee(Beneficiaries<T::AccountId>),
	}

	#[derive(DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct Swap<T: Config> {
		pub swap_id: SwapId,
		pub swap_request_id: SwapRequestId,
		pub from: Asset,
		pub to: Asset,
		pub input_amount: AssetAmount,
		pub fees: Vec<FeeType<T>>, // Moving this field to the swap request
		pub refund_params: Option<SwapRefundParameters>,
		pub execute_at: BlockNumberFor<T>,
	}

	#[derive(PartialEq, Eq, Encode, Decode)]
	#[expect(clippy::large_enum_variant)]
	pub enum SwapRequestState<T: Config> {
		UserSwap {
			price_limits_and_expiry: Option<PriceLimitsAndExpiry<T::AccountId>>,
			output_action: SwapOutputAction<T::AccountId>,
			dca_state: DcaState,
			// Adding the fees Vec here
		},
		NetworkFee,
		IngressEgressFee,
	}

	#[derive(PartialEq, Eq, Encode, Decode)]
	pub struct SwapRequest<T: Config> {
		pub id: SwapRequestId,
		pub input_asset: Asset,
		pub output_asset: Asset,
		pub state: SwapRequestState<T>,
	}

	#[frame_support::storage_alias]
	pub type SwapRequests<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, SwapRequestId, SwapRequest<T>>;

	#[frame_support::storage_alias]
	pub type ScheduledSwaps<T: Config> =
		StorageValue<Pallet<T>, BTreeMap<SwapId, Swap<T>>, ValueQuery>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let swap_request_count = old::SwapRequests::<T>::iter().count() as u64;
		let swaps_count = old::ScheduledSwaps::<T>::get().len() as u64;
		Ok((swap_request_count, swaps_count).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let old_swaps = old::ScheduledSwaps::<T>::take();

		crate::SwapRequests::<T>::translate_values::<old::SwapRequest<T>, _>(|old_swap_request| {
			let state = match old_swap_request.state {
				old::SwapRequestState::UserSwap {
					output_action,
					dca_state,
					price_limits_and_expiry,
				} => {
					let mut request_network_fee_tracker: Option<old::NetworkFeeTracker> = None;
					let mut request_beneficiaries = None;

					// Find the swaps related to this swap request to gather their fees.
					dca_state.scheduled_chunks.iter().for_each(|swap_id| {
						let (swap_network_fee_tracker, swap_beneficiaries) =
							if let Some(swap) = old_swaps.get(swap_id) {
								let tracker = swap.fees.iter().find_map(|fee| match fee {
									old::FeeType::NetworkFee(tracker) => Some(tracker.clone()),
									old::FeeType::BrokerFee(_) => None,
								});
								let beneficiaries = swap.fees.iter().find_map(|fee| match fee {
									old::FeeType::BrokerFee(beneficiaries) =>
										Some(beneficiaries.clone()),
									old::FeeType::NetworkFee(_) => None,
								});
								(tracker, beneficiaries)
							} else {
								(None, None)
							};

						// We need to merge the network fees if there is more than one swap.
						match (&mut request_network_fee_tracker, swap_network_fee_tracker) {
							(None, Some(tracker)) => {
								request_network_fee_tracker = Some(tracker);
							},
							(Some(existing_tracker), Some(tracker)) =>
								existing_tracker.merge(tracker),
							_ => {},
						}
						// The beneficiaries should be the same for all swaps
						request_beneficiaries = swap_beneficiaries.clone();
					});

					// Build the new list of fees, making sure to put network fee first.
					let mut fees = vec![];
					if let Some(tracker) = request_network_fee_tracker {
						fees.push(FeeType::NetworkFee(tracker.into()));
					}
					if let Some(beneficiaries) = request_beneficiaries {
						fees.push(FeeType::BrokerFee(beneficiaries));
					}

					SwapRequestState::UserSwap {
						output_action,
						dca_state,
						price_limits_and_expiry,
						fees,
					}
				},
				old::SwapRequestState::NetworkFee => SwapRequestState::NetworkFee,
				old::SwapRequestState::IngressEgressFee => SwapRequestState::IngressEgressFee,
			};

			Some(SwapRequest {
				id: old_swap_request.id,
				input_asset: old_swap_request.input_asset,
				output_asset: old_swap_request.output_asset,
				state,
			})
		});

		// Now we can go a delete the fees vec from the swaps and add it back into storage.
		let new_swap = old_swaps
			.into_iter()
			.map(|(id, old_swap)| {
				(
					id,
					Swap {
						swap_id: old_swap.swap_id,
						swap_request_id: old_swap.swap_request_id,
						from: old_swap.from,
						to: old_swap.to,
						input_amount: old_swap.input_amount,
						// fees removed from here
						refund_params: old_swap.refund_params,
						execute_at: old_swap.execute_at,
					},
				)
			})
			.collect::<BTreeMap<SwapId, Swap<T>>>();
		ScheduledSwaps::<T>::put(new_swap);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let (pre_swap_request_count, pre_swaps_count) = <(u64, u64)>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_swap_request_count = crate::SwapRequests::<T>::iter().count() as u64;
		let post_swaps_count = crate::ScheduledSwaps::<T>::get().len() as u64;

		// Make sure we didn't lose any swaps or swap requests.
		assert_eq!(pre_swap_request_count, post_swap_request_count);
		assert_eq!(pre_swaps_count, post_swaps_count);
		Ok(())
	}
}
