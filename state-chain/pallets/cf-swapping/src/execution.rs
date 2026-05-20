// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
	swap_state::{AfterBrokerFee4, AfterNetworkFee1, AfterSwapLegs3, SwapLegs2, SwapState},
	utilities::split_off_highest_impact_swap,
};
use cf_chains::{AccountOrAddress, CcmDepositMetadataChecked};
use cf_primitives::{
	basis_points::SignedHundredthBasisPoints, DcaParameters, ONE_AS_BASIS_POINTS, STABLE_ASSET,
};
use cf_traits::{
	lending::LendingSystemApi, AssetConverter, EgressApi, ExpiryBehaviour, FundAccount,
	FundingSource, PoolPriceProvider, PriceFeedApi, ScheduledEgressDetails, SwapExecutionProgress,
	SwapRequestType,
};
use frame_support::{storage::with_transaction_unchecked, transactional};
use sp_runtime::{
	helpers_128bit::multiply_by_rational_with_rounding, traits::UniqueSaturatedInto,
	FixedPointNumber, FixedU64, Rounding, SaturatedConversion, TransactionOutcome,
};
use sp_std::{cmp::max, collections::btree_set::BTreeSet};

use strum::IntoEnumIterator;

#[cfg(test)]
mod tests;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct SwapLeg {
	pub from: Asset,
	pub to: Asset,
}

pub enum SwapLegOutcome<T: Config> {
	Complete(SwapState<T, AfterSwapLegs3>),
	Continuing(SwapState<T, SwapLegs2>),
}

/// We split the grouping logic into 4 execution phases.
/// 1) Non-usdc native swaps (e.g. Wbtc<->Btc)
/// 2) Swaps to USDC
/// 3) Swaps from USDC
/// 4) Any remaining native swaps
///
/// This prioritizes grouping of USDC swaps while ensuring that all assets are swapped within
/// 4 legs.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, strum_macros::EnumIter)]
pub enum GroupExecutionPhase {
	InitialNativePools,
	ToUSDC,
	FromUSDC,
	FinalNativePools,
}

pub(crate) struct BatchSwapFailed<T: Config> {
	swaps: Vec<SwapState<T, SwapLegs2>>,
	amount: AssetAmount,
}

impl<T: Config> Pallet<T> {
	/// Returns all scheduled swap legs for the given base/quote pair, along with the block number
	/// they are scheduled to execute at.
	/// If safe mode is enabled, nothing will be returned since all swaps will be rescheduled.
	pub fn get_scheduled_swap_legs(
		base_asset: Asset,
		quote_asset: Asset,
	) -> Vec<(SwapLegInfo, BlockNumberFor<T>)> {
		if !T::SafeMode::get().swaps_enabled {
			return vec![];
		}

		ScheduledSwaps::<T>::get()
			.into_values()
			.flat_map(|swap| {
				let swap_request = SwapRequests::<T>::get(swap.swap_request_id)
					.expect("Swap request should exist");

				// Determine the network fee for this swap and deduct it from the input amount.
				let network_fee = Self::get_network_fee_for_swap(
					swap.from,
					swap.to,
					NetworkFeeType::from_swap_request_state(&swap_request.state),
				);
				let fee = max(network_fee.rate * swap.input_amount, network_fee.minimum);
				let input_amount_after_fees =
					swap.input_amount.saturating_sub(core::cmp::min(fee, swap.input_amount));

				// Get the DCA details from the swap request
				let (remaining_chunks, chunk_interval) = match &swap_request.state {
					SwapRequestState::UserSwap { dca_state, .. } =>
						(dca_state.remaining_chunks, dca_state.chunk_interval),
					_ => (0, SWAP_DELAY_BLOCKS),
				};

				// Check the swap route for any legs we care about
				Self::get_swap_route(swap.from, swap.to)
					.into_iter()
					.filter_map(|leg| {
						// Filter for just the base/quote pair that we are interested in
						if (leg.from == base_asset && leg.to == quote_asset) ||
							(leg.from == quote_asset && leg.to == base_asset)
						{
							return None;
						}

						// Get the input amount for this leg
						let (leg_input_amount, source_asset, source_amount) =
							if leg.from == swap.from {
								// Must be the first leg, so no need to estimate
								(input_amount_after_fees, None, None)
							} else {
								// For all other legs, estimate using oracle price with pool price
								// fallback
								let conversion_price =
									T::PriceFeedApi::get_relative_price(swap.from, leg.from)
										.map(|oracle| oracle.price)
										.or_else(|| {
											T::PoolPriceApi::pool_price(swap.from, leg.from)
												.ok()
												.map(|p| p.sell)
										})?;
								let amount = conversion_price
									.output_amount_ceil(input_amount_after_fees)
									.unique_saturated_into();
								(amount, Some(swap.from), Some(swap.input_amount))
							};

						let side = if leg.from == base_asset { Side::Sell } else { Side::Buy };

						Some((
							SwapLegInfo {
								swap_id: swap.swap_id,
								swap_request_id: swap.swap_request_id,
								base_asset,
								quote_asset,
								side,
								amount: leg_input_amount,
								source_asset,
								source_amount,
								remaining_chunks,
								chunk_interval,
							},
							swap.execute_at,
						))
					})
					.collect::<Vec<_>>()
			})
			.collect()
	}

	pub(crate) fn take_network_fees(
		swaps: Vec<SwapState<T, ()>>,
	) -> Vec<SwapState<T, AfterNetworkFee1>> {
		let mut total_network_fees = CollectedNetworkFee::<T>::get();
		let swaps_after_network_fees: Vec<SwapState<T, AfterNetworkFee1>> = swaps
			.into_iter()
			.map(|state| {
				SwapRequests::<T>::mutate(state.swap_request_id(), |swap_request| {
					// Get the network fee tracker from the swap request
					let fee_tracker = if let Some(swap_request) = swap_request {
						match &mut swap_request.state {
							SwapRequestState::UserSwap { network_fee_tracker, .. } =>
								Some(network_fee_tracker),
							SwapRequestState::IngressEgressFee |
							SwapRequestState::BrokerFee { .. } =>
							// Disposable network fee tracker with no minimum
								Some(&mut NetworkFeeTracker::new_without_minimum(
									Pallet::<T>::get_network_fee_rate_for_swap(
										state.input_asset(),
										state.output_asset(),
										NetworkFeeType::NoMinimum,
									),
								)),
							SwapRequestState::NetworkFee => None,
						}
					} else {
						log_or_panic!("Swap request {} should exist", state.swap_request_id());
						None
					};

					// Take the network fee from the input asset
					if let Some(tracker) = fee_tracker {
						let state = state.take_network_fee(tracker);
						total_network_fees
							.entry(state.input_asset())
							.or_default()
							.saturating_accrue(state.stage.network_fee_taken);
						state
					} else {
						state.no_network_fee()
					}
				})
			})
			.collect();

		// Save the updated total network fees
		CollectedNetworkFee::<T>::set(total_network_fees);

		swaps_after_network_fees
	}

	fn take_broker_fees(
		swaps: Vec<SwapState<T, AfterSwapLegs3>>,
	) -> Vec<SwapState<T, AfterBrokerFee4>> {
		// Take the broker fee from the output amount
		swaps
			.into_iter()
			.map(|swap| {
				// Get the broker fee tracker from the swap request
				SwapRequests::<T>::mutate(swap.swap_request_id(), |swap_request| {
					if let Some(swap_request) = swap_request {
						// Only user swaps have broker fees
						if let SwapRequestState::UserSwap { broker_fees_tracker, .. } =
							&mut swap_request.state
						{
							swap.take_broker_fees(broker_fees_tracker)
						} else {
							swap.no_broker_fee()
						}
					} else {
						log_or_panic!("Swap request {} should exist", swap.swap_request_id());
						swap.no_broker_fee()
					}
				})
			})
			.collect()
	}

	fn start_broker_fee_swaps_or_credit(
		fee_tracker: &BrokerFeesTracker<T::AccountId>,
		fee_asset: Asset,
	) -> BTreeMap<T::AccountId, SwapRequestId> {
		if fee_asset == Asset::Usdc {
			// No need to swap if the fee asset is already in usdc, just credit directly.
			for (Beneficiary { account, .. }, amount) in fee_tracker.iter() {
				T::BalanceApi::credit_account(account, fee_asset, *amount)
			}
			return BTreeMap::new();
		}

		// For each beneficiary start a swap to convert the fee into usdc.
		fee_tracker
			.iter()
			.filter_map(|(Beneficiary { account, .. }, amount)| {
				if *amount > 0 {
					let fee_swap_request_id =
						Self::init_broker_fee_swap_request(fee_asset, *amount, account.clone());
					Some((account.clone(), fee_swap_request_id))
				} else {
					None
				}
			})
			.collect()
	}

	/// Calculate executed price delta from the oracle price.
	///
	/// Returns signed hundredth basis points where negative means worse than oracle price (as
	/// expected for most swaps).
	///
	/// Returns:
	/// - `Ok(Some(delta))` if the oracle price is available and fresh
	/// - `Ok(None)` if the oracle price is unavailable (No prices for this asset: skip the check)
	/// - `Err(OraclePriceStale)` if the oracle price is stale
	pub(crate) fn get_delta_from_oracle_price(
		input: AssetAndAmount,
		output: AssetAndAmount,
	) -> Result<Option<SignedHundredthBasisPoints>, SwapFailureReason> {
		if input.amount == 0 {
			// Price is undefined when input is zero (would cause division by zero).
			return Ok(None);
		}
		match T::PriceFeedApi::get_relative_price(input.asset, output.asset) {
			Some(oracle_price) if oracle_price.stale => Err(SwapFailureReason::OraclePriceStale),
			Some(oracle_price) => Ok(Price::sell_price(input.amount.into(), output.amount.into())
				.map(|execution_price| {
					execution_price.hundredth_bps_difference_from(&oracle_price.price)
				})),
			None => Ok(None), // Price unavailable - skip the check
		}
	}

	/// Enforce price protections (minimum price and oracle price) and return the price delta from
	/// oracle (excluding fees) if available.
	/// A stale price on any leg of the swap will only cause a failure if the swap has refund
	/// params.
	pub(crate) fn check_swap_price_violation(
		swap: &SwapState<T, AfterBrokerFee4>,
	) -> Result<Option<SignedBasisPoints>, SwapFailureReason> {
		if let Some(params) = swap.refund_params() {
			// Minimum price protection, aka FoK price protection
			let min_price_output = params
				.price_limits
				.min_price
				.output_amount_floor(swap.input_amount_before_fees())
				.unique_saturated_into();
			if swap.stage.output_amount_after_fees < min_price_output {
				return Err(SwapFailureReason::MinPriceViolation);
			}
		}

		// Calculate the slippage from oracle prices for each leg of the swap (without being
		// affected by fees), then sum the deltas. Legs without an oracle price contribute
		// nothing rather. This way we can partially enforce oracle price protection.
		let route_amd_amounts = sp_std::iter::once(AssetAndAmount::new(
			swap.input_asset(),
			swap.stage.input_amount_after_fees,
		))
		.chain(swap.stage.intermediates.iter().copied())
		.chain(sp_std::iter::once(AssetAndAmount::new(
			swap.output_asset(),
			swap.stage.output_amount_before_fees,
		)))
		.collect::<Vec<_>>();

		let mut total_delta: Option<SignedHundredthBasisPoints> = None;
		for leg in route_amd_amounts.windows(2) {
			let leg_delta = match Self::get_delta_from_oracle_price(leg[0], leg[1]) {
				// A stale oracle price only fails the check when the swap has refund params. This
				// way the swap simulation can still run with stale prices.
				Err(SwapFailureReason::OraclePriceStale) if swap.refund_params().is_none() =>
					return Ok(None),
				other => other?,
			};
			if let Some(leg_delta) = leg_delta {
				total_delta = Some(match total_delta {
					Some(acc) => acc.saturating_add(&leg_delta),
					None => leg_delta,
				});
			}
		}

		if let Some(total_delta) = total_delta {
			// Oracle price protection, aka Live price protection (LPP)
			if let Some(params) = swap.refund_params() {
				if let Some(max_slippage) = params.price_limits.max_oracle_price_slippage {
					// The swapper expresses the limit as a worst acceptable *sell*
					// price, so slippage needs to be measured in the negative
					// direction (lower sell price is worse).
					if total_delta < SignedBasisPoints::negative_slippage(max_slippage).into() {
						return Err(SwapFailureReason::OraclePriceSlippageExceeded);
					}
				}
			}

			return Ok(Some(total_delta.pessimistic_rounded_into()));
		}

		Ok(None)
	}

	#[transactional]
	pub(crate) fn try_execute_without_violations(
		swaps: BTreeMap<SwapId, Swap<T>>,
	) -> Result<Vec<SuccessfulSwap>, BatchExecutionError<T>> {
		// Bundle each swap with a fresh swap state
		let swap_states: Vec<_> = swaps.values().cloned().map(SwapState::new).collect();
		// Take the network fee
		let swap_states = Self::take_network_fees(swap_states);
		// Run the actual swaps
		let swap_states = Self::execute_all_swap_legs(swap_states)?;
		// Take the broker fees
		let swap_states = Self::take_broker_fees(swap_states);

		// Successfully executed without hitting price impact limit.
		// Now check for price violations (oracle and minimum price).
		let mut non_violating_swaps = vec![];
		let mut violating_swaps = vec![];
		swap_states.into_iter().for_each(|swap| match swap.check_for_price_violation() {
			Ok(swap) => {
				non_violating_swaps.push(swap);
			},
			Err((state, reason)) => {
				violating_swaps.push((state, reason));
			},
		});

		if violating_swaps.is_empty() {
			// Final step of the swapping process is to calculate the oracle delta.
			Ok(non_violating_swaps
				.into_iter()
				.map(|swap| swap.calculate_oracle_delta())
				.collect())
		} else {
			Err(BatchExecutionError::PriceViolation {
				violating_swaps,
				non_violating_swaps: non_violating_swaps
					.into_iter()
					.map(SwapState::into_swap)
					.collect(),
			})
		}
	}

	pub fn simulate_swap(
		input_asset: Asset,
		output_asset: Asset,
		amount: AssetAmount,
		network_fee: FeeRateAndMinimum,
		broker_fees: Beneficiaries<T::AccountId>,
	) -> Result<SuccessfulSwap, BatchExecutionError<T>> {
		with_transaction_unchecked(|| {
			TransactionOutcome::Rollback({
				const SWAP_REQUEST_ID: SwapRequestId = SwapRequestId(0);
				let swap_id: SwapId = SwapId::from(0);

				// Create a dummy swap request to hold the fee information
				let swap_request = SwapRequest {
					id: SWAP_REQUEST_ID,
					input_asset,
					output_asset,
					state: SwapRequestState::UserSwap {
						price_limits_and_expiry: None,
						output_action: SwapOutputAction::Egress {
							ccm_deposit_metadata: None,
							output_address: ForeignChainAddress::Eth([0; 20].into()),
						},
						dca_state: DcaState {
							scheduled_chunks: BTreeSet::from([(swap_id)]),
							remaining_input_amount: amount,
							remaining_chunks: 0,
							chunk_interval: SWAP_DELAY_BLOCKS,
							accumulated_output_amount: 0,
						},
						network_fee_tracker: NetworkFeeTracker::new(network_fee),
						broker_fees_tracker: BrokerFeesTracker::new(broker_fees),
					},
				};
				SwapRequests::<T>::insert(SWAP_REQUEST_ID, swap_request);

				// Create the swap and try to execute it
				let swap = Swap::new(
					swap_id,
					SWAP_REQUEST_ID,
					input_asset,
					output_asset,
					amount,
					None,               // Refund params
					Default::default(), // Execution block
				);

				// We expect to get exactly one swap back if the execution is successful.
				Self::try_execute_without_violations(BTreeMap::from([(swap.swap_id, swap)]))
					.and_then(|swaps| {
						swaps
							.into_iter()
							.next()
							.ok_or(DispatchError::Other("Unexpected empty swap result").into())
					})
			})
		})
	}

	/// Attempts to find (and execute) a batch of swaps that wouldn't result in hitting the
	/// price impact limit, starting with the given batch, and taking swaps out of the batch if
	/// needed.
	pub(crate) fn execute_batch(
		mut swaps_to_execute: BTreeMap<SwapId, Swap<T>>,
	) -> BatchExecutionOutcomes<T> {
		let mut failed_swaps = vec![];

		loop {
			if swaps_to_execute.is_empty() {
				return BatchExecutionOutcomes { successful_swaps: vec![], failed_swaps };
			}

			match Self::try_execute_without_violations(swaps_to_execute.clone()) {
				Ok(successful_swaps) =>
					return BatchExecutionOutcomes { successful_swaps, failed_swaps },
				Err(BatchExecutionError::SwapLegFailed {
					from_asset,
					to_asset,
					amount,
					failed_swap_group,
				}) => {
					Self::deposit_event(Event::<T>::BatchSwapFailed {
						asset: if from_asset == STABLE_ASSET { to_asset } else { from_asset },
						direction: if from_asset == STABLE_ASSET {
							LegacySwapLegDirection::FromStable
						} else {
							LegacySwapLegDirection::ToStable
						},
						amount,
					});

					// Find the largest swap from the failing pool/direction and remove it
					// so we can try the remaining swaps again. We should always be able to
					// find a swap to remove, but if we can't for some reason, abort.
					if let Some(removed_swap) =
						split_off_highest_impact_swap(&mut swaps_to_execute, failed_swap_group)
					{
						failed_swaps.push((removed_swap, SwapFailureReason::PriceImpactLimit));
					} else {
						break;
					}
				},
				Err(BatchExecutionError::PriceViolation {
					violating_swaps,
					non_violating_swaps,
				}) => {
					failed_swaps.extend(violating_swaps);
					swaps_to_execute = non_violating_swaps
						.into_iter()
						.map(|swap| {
							(
								swap.swap_id,
								swaps_to_execute
									.get(&swap.swap_id)
									.expect("must be subset of given swaps")
									.clone(),
							)
						})
						.collect();
				},
				Err(BatchExecutionError::DispatchError { error }) => {
					// This should only happen when the transaction nested too deep,
					// which should not happen in practice (max nesting is 255):
					log_or_panic!("Failed to execute swap batch: {error:?}");
					break;
				},
			}
		}

		// If we are here, consider all swaps as failed:
		failed_swaps.extend(
			swaps_to_execute
				.into_values()
				.map(|swap| (swap, SwapFailureReason::PriceImpactLimit)),
		);
		BatchExecutionOutcomes { successful_swaps: vec![], failed_swaps }
	}

	pub(crate) fn refund_failed_swap(swap: Swap<T>, reason: SwapFailureReason) {
		let swap_request_id = swap.swap_request_id;

		Self::deposit_event(Event::<T>::SwapAborted { swap_id: swap.swap_id, reason });

		let Some(mut request) = SwapRequests::<T>::take(swap_request_id) else {
			log_or_panic!("Swap request {swap_request_id} not found");
			return;
		};

		let broker_fee_swaps = match &mut request.state {
			SwapRequestState::UserSwap {
				output_action,
				price_limits_and_expiry,
				dca_state,
				broker_fees_tracker,
				network_fee_tracker,
			} => {
				let Some(ExpiryBehaviour::RefundIfExpires {
					refund_address,
					refund_ccm_metadata,
					..
				}) = price_limits_and_expiry.as_ref().map(|p| &p.expiry_behaviour)
				else {
					log_or_panic!(
						"Trying to refund swap request {swap_request_id}, but missing refund parameters"
					);
					return;
				};

				// Cancel any other scheduled swaps for this swap request and add the amounts
				// back to the input remaining.
				let canceled_swaps_amount = dca_state
					.scheduled_chunks
					.iter()
					.filter(|swap_id| *swap_id != &swap.swap_id)
					.fold(0, |acc: u128, swap_id| {
						acc.saturating_add(Self::cancel_swap(
							*swap_id,
							SwapFailureReason::PredecessorSwapFailure,
						))
					});

				// Take the network fee for the chunk that failed
				let remaining_amount = CollectedNetworkFee::<T>::mutate(|total_fees| {
					let FeeTaken { remaining_amount, fee } =
						network_fee_tracker.take_fee(swap.input_amount);
					total_fees.entry(request.input_asset).or_default().saturating_accrue(fee);
					remaining_amount
				});

				let total_input_remaining =
					remaining_amount + dca_state.remaining_input_amount + canceled_swaps_amount;

				if total_input_remaining > 0 {
					match refund_address {
						AccountOrAddress::ExternalAddress(address) => {
							Self::egress_for_swap(
								request.id,
								total_input_remaining,
								request.input_asset,
								address.clone(),
								refund_ccm_metadata.clone(),
								EgressType::Refund,
							);
						},
						AccountOrAddress::InternalAccount(account_id) => {
							Self::deposit_event(Event::<T>::RefundedOnChain {
								swap_request_id,
								account_id: account_id.clone(),
								asset: request.input_asset,
								amount: total_input_remaining,
							});

							T::BalanceApi::credit_account(
								account_id,
								request.input_asset,
								total_input_remaining,
							);
						},
					}
				} else {
					Self::deposit_event(Event::<T>::RefundEgressIgnored {
						swap_request_id,
						asset: request.input_asset,
						amount: total_input_remaining,
						reason: DispatchError::from(Error::<T>::NoRefundAmountRemaining),
					});
				}

				// In case of DCA we may have partially swapped and now have some output
				// asset to egress to the output address:
				if dca_state.accumulated_output_amount > 0 {
					match output_action {
						SwapOutputAction::Egress { ccm_deposit_metadata, output_address } => {
							Self::egress_for_swap(
								swap_request_id,
								dca_state.accumulated_output_amount,
								request.output_asset,
								output_address.clone(),
								ccm_deposit_metadata.clone(),
								EgressType::Regular,
							);
						},
						SwapOutputAction::CreditOnChain { account_id } => {
							Self::deposit_event(Event::<T>::CreditedOnChain {
								swap_request_id,
								account_id: account_id.clone(),
								asset: request.output_asset,
								amount: dca_state.accumulated_output_amount,
							});

							T::BalanceApi::credit_account(
								account_id,
								request.output_asset,
								dca_state.accumulated_output_amount,
							);
						},
						SwapOutputAction::CreditLendingPool { swap_type } => {
							log_or_panic!("Unexpected refund of a loan swap: {swap_type:?}");
						},
						SwapOutputAction::CreditFlipAndTransferToGateway { .. } => {
							log_or_panic!(
								"Unexpected refund of initial funding swap: {swap_request_id:?}"
							);
						},
					}
				}

				// Start swaps for any accumulated broker fees
				Self::start_broker_fee_swaps_or_credit(broker_fees_tracker, request.output_asset)
			},
			non_refundable_request => {
				log_or_panic!(
					"Refund for swap request is not supported: {non_refundable_request:?}"
				);
				Default::default()
			},
		};
		Self::deposit_event(Event::<T>::SwapRequestCompleted {
			swap_request_id: request.id,
			reason: SwapRequestCompletionReason::Expired,
			broker_fee_swaps,
		});
	}

	// Removes the swap from the scheduled swaps and returns the input amount of the canceled
	// swap.
	pub(crate) fn cancel_swap(swap_id: SwapId, reason: SwapFailureReason) -> AssetAmount {
		ScheduledSwaps::<T>::mutate(|swaps| {
			let amount = swaps.remove(&swap_id).map(|swap| {
				Self::deposit_event(Event::<T>::SwapAborted { swap_id: swap.swap_id, reason });
				swap.input_amount
			});
			if amount.is_none() {
				log_or_panic!(
					"Attempted to cancel swap {swap_id}, but it was not found in ScheduledSwaps"
				);
			}
			amount.unwrap_or_default()
		})
	}

	pub(crate) fn process_swap_outcome(swap: SuccessfulSwap) {
		let swap_request_id = swap.swap_request_id;

		let Some(mut request) = SwapRequests::<T>::take(swap_request_id) else {
			log_or_panic!("Swap request {swap_request_id} not found");
			return;
		};

		Self::deposit_event(Event::<T>::SwapExecuted {
			swap_request_id,
			swap_id: swap.swap_id,
			input: AssetAndAmount::new(swap.input_asset, swap.input_amount_after_fees),
			network_fee: AssetAndAmount::new(swap.input_asset, swap.network_fee_taken),
			broker_fee: AssetAndAmount::new(swap.output_asset, swap.broker_fee_taken),
			output: AssetAndAmount::new(swap.output_asset, swap.output_amount_after_fees),
			intermediates: swap.intermediates,
			oracle_delta: swap.oracle_delta,
			oracle_delta_ex_fees: swap.oracle_delta_ex_fees,
		});

		let (request_completed, broker_fee_swaps) = match &mut request.state {
			SwapRequestState::UserSwap {
				output_action,
				dca_state,
				price_limits_and_expiry,
				broker_fees_tracker,
				..
			} =>
				if let Some(chunk_input_amount) = dca_state.calculate_next_chunk() {
					let swap_id = Self::schedule_swap(
						request.input_asset,
						request.output_asset,
						chunk_input_amount,
						price_limits_and_expiry.as_ref(),
						SwapType::Swap,
						request.id,
						// Schedule the next chunk to be after any currently scheduled chunks
						(dca_state.scheduled_chunks.len() as u32)
							.saturating_mul(dca_state.chunk_interval)
							.into(),
					);

					dca_state.record_scheduled_chunk(swap_id, chunk_input_amount);
					dca_state.record_chunk_completion(swap.swap_id, swap.output_amount_after_fees);

					(false, Default::default())
				} else {
					debug_assert!(dca_state.remaining_input_amount == 0);

					dca_state.record_chunk_completion(swap.swap_id, swap.output_amount_after_fees);

					// This may or may not be the last chunk (some may already be scheduled)
					if dca_state.scheduled_chunks.is_empty() {
						match output_action {
							SwapOutputAction::Egress { ccm_deposit_metadata, output_address } => {
								Self::egress_for_swap(
									swap_request_id,
									dca_state.accumulated_output_amount,
									swap.output_asset,
									output_address.clone(),
									ccm_deposit_metadata.clone(),
									EgressType::Regular,
								);
							},
							SwapOutputAction::CreditOnChain { account_id } => {
								Self::deposit_event(Event::<T>::CreditedOnChain {
									swap_request_id,
									account_id: account_id.clone(),
									asset: request.output_asset,
									amount: dca_state.accumulated_output_amount,
								});

								T::BalanceApi::credit_account(
									account_id,
									request.output_asset,
									dca_state.accumulated_output_amount,
								);
							},
							SwapOutputAction::CreditLendingPool { swap_type } => {
								T::LendingSystemApi::process_loan_swap_outcome(
									swap_request_id,
									swap_type.clone(),
									dca_state.accumulated_output_amount,
								);
							},
							SwapOutputAction::CreditFlipAndTransferToGateway {
								account_id,
								flip_to_subtract_from_swap_output,
							} =>
								if request.output_asset == Asset::Flip {
									if swap.output_amount_after_fees <
										*flip_to_subtract_from_swap_output
									{
										// In the rare event that this occurs we will track the
										// deficit and offset it against the next burn
										FlipToBurn::<T>::mutate(|total| {
											total.saturating_reduce(
												flip_to_subtract_from_swap_output
													.saturating_sub(swap.output_amount_after_fees)
													.try_into()
													.unwrap_or(i128::MAX),
											);
										});
										FlipToBeSentToGateway::<T>::mutate(|total| {
											total.saturating_accrue(
												*flip_to_subtract_from_swap_output,
											);
										});
									} else {
										T::FundAccount::fund_account(
											account_id.clone(),
											swap.output_amount_after_fees
												.saturating_sub(*flip_to_subtract_from_swap_output)
												.into(),
											FundingSource::Swap { swap_request_id },
										);
										FlipToBeSentToGateway::<T>::mutate(|total| {
											total.saturating_accrue(swap.output_amount_after_fees);
										});
									}
								} else {
									log_or_panic!(
										"Encountered transfer to gateway swap for asset that isn't Flip: {swap_request_id:?}"
									);
								},
						}

						// Start swaps for any accumulated broker fees
						let broker_fee_swaps = Self::start_broker_fee_swaps_or_credit(
							broker_fees_tracker,
							request.output_asset,
						);

						(true, broker_fee_swaps)
					} else {
						(false, Default::default())
					}
				},
			SwapRequestState::NetworkFee => {
				if swap.output_asset == Asset::Flip {
					FlipToBurn::<T>::mutate(|total| {
						total.saturating_accrue(
							swap.output_amount_after_fees.try_into().unwrap_or(i128::MAX),
						);
					});
				} else {
					log_or_panic!(
						"NetworkFee burning should not be in asset: {:?}",
						swap.output_asset
					);
				}
				(true, Default::default())
			},
			SwapRequestState::IngressEgressFee => {
				if swap.output_asset == ForeignChain::from(swap.output_asset).gas_asset() {
					T::IngressEgressFeeHandler::accrue_withheld_fee(
						swap.output_asset,
						swap.output_amount_after_fees,
					);
				} else {
					log_or_panic!(
						"IngressEgressFee swap should not be to non-gas asset: {:?}",
						swap.output_asset
					);
				}

				(true, Default::default())
			},
			SwapRequestState::BrokerFee { account_id } => {
				T::BalanceApi::credit_account(
					account_id,
					swap.output_asset,
					swap.output_amount_after_fees,
				);

				(true, Default::default())
			},
		};

		if request_completed {
			Self::deposit_event(Event::<T>::SwapRequestCompleted {
				swap_request_id,
				reason: SwapRequestCompletionReason::Executed,
				broker_fee_swaps,
			});
		} else {
			SwapRequests::<T>::insert(swap_request_id, request);
		}
	}

	fn advance_swap_with_leg_output(
		swap_state: SwapState<T, SwapLegs2>,
		swap_leg_output: AssetAndAmount,
	) -> SwapLegOutcome<T> {
		if swap_leg_output.asset == swap_state.output_asset() {
			// This was the final leg, so the swap is complete
			SwapLegOutcome::Complete(swap_state.finished_swap_legs(swap_leg_output))
		} else {
			// This was an intermediate leg, so the swap is continuing
			SwapLegOutcome::Continuing(swap_state.advance_swap_leg(swap_leg_output))
		}
	}

	/// Determines the route a swap will take. Returning a vector of all the swap legs.
	pub(crate) fn get_swap_route(input_asset: Asset, output_asset: Asset) -> Vec<SwapLeg> {
		let mut route = Vec::new();
		let mut current_asset = input_asset;

		for _ in GroupExecutionPhase::iter() {
			if let Some(next_leg) = Self::get_next_swap_leg(current_asset, output_asset) {
				current_asset = next_leg.to;
				route.push(next_leg);
			} else {
				break;
			}
		}

		route
	}

	// Routing logic. Determines the next swap leg needed to get from the input asset to the output
	// asset,
	pub fn get_next_swap_leg(input_asset: Asset, output_asset: Asset) -> Option<SwapLeg> {
		match (input_asset, output_asset) {
			// No swap needed if the assets are the same
			(input, output) if input == output => None,
			// Use the non-usdc native swap pools first
			(Asset::Btc, Asset::Wbtc) | (Asset::Wbtc, Asset::Btc) if ENABLE_WBTC_BTC_ROUTE =>
				Some(SwapLeg { from: input_asset, to: output_asset }),
			// If either asset is the stable asset, we can swap directly between them
			(Asset::Usdc, _) | (_, Asset::Usdc) =>
				Some(SwapLeg { from: input_asset, to: output_asset }),
			// If none of the above conditions are met, then we must swap through the stable asset
			// as an intermediate.
			_ => Some(SwapLeg { from: input_asset, to: Asset::Usdc }),
		}
	}

	// Grouping logic. Returns true if the swap leg should be executed in
	// this iteration or false if it should be delayed to a later iteration.
	pub fn should_group_leg_execute(
		from_asset: Asset,
		to_asset: Asset,
		phase: GroupExecutionPhase,
	) -> bool {
		match (phase, from_asset, to_asset) {
			// First we swap the Wbtc/Btc native swaps
			(GroupExecutionPhase::InitialNativePools, from, to)
				if (to == Asset::Wbtc && from == Asset::Btc) ||
					(to == Asset::Btc && from == Asset::Wbtc) =>
				true,
			// Next we swap to USDC
			(GroupExecutionPhase::ToUSDC, _, Asset::Usdc) => true,
			// Then we swap from USDC
			(GroupExecutionPhase::FromUSDC, Asset::Usdc, _) => true,
			// Finally we swap any remaining Wbtc/Btc native swaps
			(GroupExecutionPhase::FinalNativePools, from, to)
				if (to == Asset::Wbtc && from == Asset::Btc) ||
					(to == Asset::Btc && from == Asset::Wbtc) =>
				true,
			// Delay the swaps until a later leg
			_ => false,
		}
	}

	/// Runs all 4 possible swap legs of all swaps with grouping logic
	pub(crate) fn execute_all_swap_legs(
		swaps: Vec<SwapState<T, AfterNetworkFee1>>,
	) -> Result<Vec<SwapState<T, AfterSwapLegs3>>, BatchExecutionError<T>> {
		let mut completed_swaps = Vec::new();
		let mut continuing_swaps = Vec::new();

		// Skip execution for swaps that are already in the final asset and prepare the rest for
		// execution
		for swap_state in swaps {
			if swap_state.input_asset() == swap_state.output_asset() {
				completed_swaps.push(swap_state.proceed_without_execution());
			} else {
				continuing_swaps.push(swap_state.prepare_for_execution());
			}
		}

		// Run all 4 phases of swap leg execution
		for phase in GroupExecutionPhase::iter() {
			let mut leg_outcomes = Vec::new();
			let mut all_swap_leg_groups = BTreeMap::<SwapLeg, Vec<SwapState<T, SwapLegs2>>>::new();

			// First split the swaps into groups based on what the next swap leg for each swap is
			for swap_state in continuing_swaps.drain(..) {
				if let Some(swap_group_pair) =
					Self::get_next_swap_leg(swap_state.swap_asset(), swap_state.output_asset())
				{
					all_swap_leg_groups.entry(swap_group_pair).or_default().push(swap_state);
				} else {
					// If the swap doesn't need to be swapped because it is already in the
					// final asset, just advance with no swap output.
					leg_outcomes.push(SwapLegOutcome::Continuing(swap_state));
				}
			}

			// Use the grouping logic to determine which swap legs should be executed in this
			let groups_to_execute: BTreeMap<SwapLeg, Vec<SwapState<T, SwapLegs2>>> =
				all_swap_leg_groups
					.into_iter()
					.filter_map(|(swap_leg, swaps)| {
						if !Self::should_group_leg_execute(swap_leg.from, swap_leg.to, phase) {
							// If we're not executing this group yet, just advance the swaps with no
							// output
							swaps.into_iter().for_each(|swap_state| {
								leg_outcomes.push(SwapLegOutcome::Continuing(swap_state));
							});
							None
						} else {
							Some((swap_leg, swaps))
						}
					})
					.collect();

			// Execute just the groups that we determined should be executed in this leg
			for (swap_leg, swaps) in groups_to_execute {
				leg_outcomes.extend(
					Self::execute_group_of_swaps(swaps, swap_leg.clone()).map_err(
						|BatchSwapFailed { swaps: failed_swaps, amount }| {
							BatchExecutionError::SwapLegFailed {
								from_asset: swap_leg.from,
								to_asset: swap_leg.to,
								amount,
								failed_swap_group: failed_swaps
									.into_iter()
									.map(|swap| swap.failed_swap())
									.collect(),
							}
						},
					)?,
				);
			}

			// We have now processed all swaps, split off the completed ones and continue to the
			// next leg
			debug_assert!(
				continuing_swaps.is_empty(),
				"All continuing swaps should have been processed into leg_outcomes"
			);
			leg_outcomes.into_iter().for_each(|outcome| match outcome {
				SwapLegOutcome::Complete(swap_state) => completed_swaps.push(swap_state),
				SwapLegOutcome::Continuing(swap_state) => continuing_swaps.push(swap_state),
			});
		}

		debug_assert!(
			continuing_swaps.is_empty(),
			"All swaps should have been completed after 4 legs of swapping"
		);
		Ok(completed_swaps)
	}

	/// Bundle the given swaps and do a single swap of a given swap group pair.
	pub(crate) fn execute_group_of_swaps(
		swaps: Vec<SwapState<T, SwapLegs2>>,
		swap_group_pair: SwapLeg,
	) -> Result<Vec<SwapLegOutcome<T>>, BatchSwapFailed<T>> {
		debug_assert!(
			!swaps.is_empty(),
			"The implementation of grouped_swaps ensures that the swap groups are non-empty."
		);

		let bundle_input: AssetAmount = swaps.iter().map(|swap| swap.swap_amount()).sum();

		// Process the swap leg as a bundle
		if let Ok(bundle_output) =
			T::SwappingApi::swap_single_leg(swap_group_pair.from, swap_group_pair.to, bundle_input)
		{
			// Split the bundle output up to each swap and update the swap state accordingly
			let number_of_swaps = swaps.len();
			let mut total_assigned_output: AssetAmount = 0;
			Ok(swaps
				.into_iter()
				.enumerate()
				.map(|(index, swap)| {
					let swap_output = if index == number_of_swaps - 1 {
						// Give the dust to the last swap to ensure all output is assigned
						bundle_output.saturating_sub(total_assigned_output)
					} else {
						let swap_output = if bundle_input > 0 {
							multiply_by_rational_with_rounding(
								swap.swap_amount(),
								bundle_output,
								bundle_input,
								Rounding::Down,
							)
							.expect(
								"bundle_input >= swap_amount && bundle_input != 0 ∴ result can't overflow",
							)
						} else {
							0
						};

						total_assigned_output.saturating_accrue(swap_output);
						swap_output
					};
					Self::advance_swap_with_leg_output(
						swap,
						AssetAndAmount::new(swap_group_pair.to, swap_output),
					)
				})
				.collect())
		} else {
			Err(BatchSwapFailed { swaps, amount: bundle_input })
		}
	}

	pub(crate) fn schedule_swap(
		input_asset: Asset,
		output_asset: Asset,
		input_amount: AssetAmount,
		price_limits_and_expiry: Option<&PriceLimitsAndExpiry<T::AccountId>>,
		swap_type: SwapType,
		swap_request_id: SwapRequestId,
		delay_blocks: BlockNumberFor<T>,
	) -> SwapId {
		let swap_id = SwapIdCounter::<T>::mutate(|id| {
			id.saturating_accrue(1);
			*id
		});

		let execute_at = frame_system::Pallet::<T>::block_number() + delay_blocks;

		let refund_params = price_limits_and_expiry.map(|params| {
			use sp_runtime::traits::UniqueSaturatedInto;

			let execute_at: cf_primitives::BlockNumber = execute_at.unique_saturated_into();

			SwapRefundParameters {
				refund_block: if let ExpiryBehaviour::RefundIfExpires { retry_duration, .. } =
					&params.expiry_behaviour
				{
					execute_at.saturating_add(*retry_duration)
				} else {
					u32::MAX
				},
				price_limits: PriceLimits {
					min_price: params.min_price,
					max_oracle_price_slippage: params.max_oracle_price_slippage,
				},
			}
		});

		ScheduledSwaps::<T>::mutate(|swaps| {
			swaps.insert(
				swap_id,
				Swap::new(
					swap_id,
					swap_request_id,
					input_asset,
					output_asset,
					input_amount,
					refund_params,
					execute_at,
				),
			)
		});

		Self::deposit_event(Event::<T>::SwapScheduled {
			swap_request_id,
			swap_id,
			input_amount,
			swap_type,
			execute_at,
		});

		swap_id
	}

	pub(crate) fn reschedule_swap(
		mut swap: Swap<T>,
		retry_delay: BlockNumberFor<T>,
		reason: SwapFailureReason,
	) {
		SwapRequests::<T>::mutate(swap.swap_request_id, |request| {
			if let Some(request) = request {
				ScheduledSwaps::<T>::mutate(|swaps| {
					// Reschedule the main swap that just failed (it was taken from the storage
					// and needs to be put back):
					let execute_at = swap.execute_at.saturating_add(retry_delay);
					let main_swap_id = swap.swap_id;
					swap.execute_at = execute_at;
					swaps.insert(main_swap_id, swap);
					Self::deposit_event(Event::<T>::SwapRescheduled {
						swap_id: main_swap_id,
						execute_at,
						reason,
					});

					// For multi-chunk/DCA swaps (currently only user swaps can be DCA), also
					// reschedule any other chunks that may have been scheduled previously (but
					// haven't been processed yet):
					if let SwapRequestState::UserSwap { dca_state, .. } = &mut request.state {
						for swap_id in dca_state.scheduled_chunks.iter().copied() {
							if swap_id != main_swap_id {
								if let Some(s) = swaps.get_mut(&swap_id) {
									s.execute_at.saturating_accrue(retry_delay);
									Self::deposit_event(Event::<T>::SwapRescheduled {
										swap_id,
										execute_at: s.execute_at,
										reason: SwapFailureReason::PredecessorSwapFailure,
									});
								} else {
									log_or_panic!(
										"Swap {swap_id} not found in ScheduledSwaps for rescheduling",
									);
								}
							}
						}
					}
				})
			} else {
				log_or_panic!("Swap request {} not found for rescheduling", swap.swap_request_id);
			}
		});
	}

	fn estimate_usdc_price_using_simulated_swap_or_fallback(asset: Asset, side: Side) -> Price {
		const ESTIMATION_AMOUNT_USDC: u128 = 20_000_000; // 20 USDC
		match side {
			// Buy means we buy Asset and sell USDC
			Side::Buy => {
				// Estimated Asset amount
				with_transaction_unchecked(|| {
					TransactionOutcome::Rollback(T::SwappingApi::swap_single_leg(
						STABLE_ASSET,
						asset,
						ESTIMATION_AMOUNT_USDC,
					))
				})
				.ok()
				.filter(|v| *v > 0)
				// Return USDC / Asset
				.and_then(|estimation_output| {
					Price::from_amounts(ESTIMATION_AMOUNT_USDC.into(), estimation_output.into())
				})
				.unwrap_or_else(|| utilities::hard_coded_price_for_asset(asset))
			},
			// Sell means we sell Asset and buy USDC
			Side::Sell => {
				let estimated_input = utilities::hard_coded_price_for_asset(asset) // USD / Asset
					// How much input is required for the output?
					.input_amount_floor(ESTIMATION_AMOUNT_USDC) // Asset
					.saturated_into();
				with_transaction_unchecked(|| {
					TransactionOutcome::Rollback(T::SwappingApi::swap_single_leg(
						asset,
						STABLE_ASSET,
						estimated_input,
					))
				})
				.ok()
				.filter(|v| *v > 0)
				// Return USDC / Asset
				.and_then(|estimation_output| {
					Price::from_amounts(estimation_output.into(), estimated_input.into())
				})
				.unwrap_or_else(|| utilities::hard_coded_price_for_asset(asset))
			},
		}
	}

	fn egress_for_swap(
		swap_request_id: SwapRequestId,
		amount: AssetAmount,
		asset: Asset,
		address: ForeignChainAddress,
		maybe_ccm_metadata: Option<CcmDepositMetadataChecked<ForeignChainAddress>>,
		egress_type: EgressType,
	) {
		match T::EgressHandler::schedule_egress(asset, amount, address, maybe_ccm_metadata) {
			Ok(ScheduledEgressDetails { egress_id, egress_amount, fee_withheld }) =>
				match egress_type {
					EgressType::Regular => Self::deposit_event(Event::<T>::SwapEgressScheduled {
						swap_request_id,
						egress_id,
						asset,
						amount: egress_amount,
						egress_fee: (fee_withheld, asset),
					}),
					EgressType::Refund => Self::deposit_event(Event::<T>::RefundEgressScheduled {
						swap_request_id,
						egress_id,
						asset,
						amount: egress_amount,
						egress_fee: (fee_withheld, asset),
					}),
				},
			Err(err) => match egress_type {
				EgressType::Regular => {
					Self::deposit_event(Event::<T>::SwapEgressIgnored {
						swap_request_id,
						asset,
						amount,
						reason: err.into(),
					});
				},
				EgressType::Refund => Self::deposit_event(Event::<T>::RefundEgressIgnored {
					swap_request_id,
					asset,
					amount,
					reason: err.into(),
				}),
			},
		}
	}

	/// Returns the configured default oracle price slippage protection for a single pool leg.
	/// Returns `None` if the asset has no oracle price feed or no valid pool pair.
	pub fn default_oracle_lpp_for_asset(leg: SwapLeg) -> Option<BasisPoints> {
		match (leg.from, leg.to) {
			(Asset::Btc, Asset::Wbtc) | (Asset::Wbtc, Asset::Btc) if ENABLE_WBTC_BTC_ROUTE =>
				Some(DefaultOraclePriceSlippageProtection::<T>::get(AssetPair::new(
					Asset::Wbtc,
					Asset::Btc,
				)?)),
			(Asset::Usdc, asset) | (asset, Asset::Usdc) => {
				if T::PriceFeedApi::get_price(asset).is_none() {
					return None;
				}
				Some(DefaultOraclePriceSlippageProtection::<T>::get(AssetPair::new(
					asset,
					Asset::Usdc,
				)?))
			},
			_ => None,
		}
	}

	/// Returns the default price protection to apply to a swap request.
	///
	/// Returns `None` if no oracle price is available for any leg of the swap (no
	/// meaningful protection can be applied). For multi-leg swaps where only some legs
	/// have an oracle, the other legs contribute zero to the total limit so that
	/// the oracle-priced legs are still protected.
	fn get_default_oracle_price_protection(
		input_asset: Asset,
		output_asset: Asset,
	) -> Option<BasisPoints> {
		// TODO JAMIE: needs to be updated with changes to behaviour from PR #6586
		let total_lpp =
			Self::get_swap_route(input_asset, output_asset)
				.into_iter()
				.fold(0u16, |acc, leg| {
					Self::default_oracle_lpp_for_asset(leg)
						.map(|lpp| acc.saturating_add(lpp))
						.unwrap_or(acc)
				});

		if total_lpp > 0 {
			Some(total_lpp)
		} else {
			None
		}
	}

	/// Gets the network fee rate and minimum in usdc terms for a swap between the given input
	/// and output assets, taking into account whether it's an internal swap or not.
	fn get_network_fee(
		input_asset: Asset,
		output_asset: Asset,
		fee_type: NetworkFeeType,
	) -> FeeRateAndMinimum {
		let (input_asset_fee, output_asset_fee, usdc_minimum) = match fee_type {
			NetworkFeeType::Internal => {
				let default_fee = InternalSwapNetworkFee::<T>::get();
				(
					InternalSwapNetworkFeeForAsset::<T>::get(input_asset)
						.unwrap_or(default_fee.rate),
					InternalSwapNetworkFeeForAsset::<T>::get(output_asset)
						.unwrap_or(default_fee.rate),
					default_fee.minimum,
				)
			},
			NetworkFeeType::Standard => {
				let default_fee = NetworkFee::<T>::get();
				(
					NetworkFeeForAsset::<T>::get(input_asset).unwrap_or(default_fee.rate),
					NetworkFeeForAsset::<T>::get(output_asset).unwrap_or(default_fee.rate),
					default_fee.minimum,
				)
			},
			NetworkFeeType::NoMinimum => {
				let default_fee = NetworkFee::<T>::get();
				(
					NetworkFeeForAsset::<T>::get(input_asset).unwrap_or(default_fee.rate),
					NetworkFeeForAsset::<T>::get(output_asset).unwrap_or(default_fee.rate),
					0_u128,
				)
			},
			NetworkFeeType::None => (Permill::zero(), Permill::zero(), 0_u128),
		};

		FeeRateAndMinimum { rate: input_asset_fee.max(output_asset_fee), minimum: usdc_minimum }
	}

	fn get_network_fee_rate_for_swap(
		input_asset: Asset,
		output_asset: Asset,
		fee_type: NetworkFeeType,
	) -> Permill {
		Self::get_network_fee(input_asset, output_asset, fee_type).rate
	}

	/// Gets the network fee rate and minimum in the input asset terms.
	pub fn get_network_fee_for_swap(
		input_asset: Asset,
		output_asset: Asset,
		fee_type: NetworkFeeType,
	) -> FeeRateAndMinimum {
		// Find the correct fee values in USDC
		let FeeRateAndMinimum { rate, minimum: usdc_minimum } =
			Self::get_network_fee(input_asset, output_asset, fee_type);

		// Convert the minimum amount to the input asset
		let minimum = if fee_type.has_minimum() {
			Pallet::<T>::calculate_input_for_desired_output_or_default_to_zero(
				input_asset,
				Asset::Usdc,
				usdc_minimum,
				false, // no network fee
			)
		} else {
			0
		};

		FeeRateAndMinimum { rate, minimum }
	}
}

impl<T: Config> SwapRequestHandler for Pallet<T> {
	type AccountId = T::AccountId;

	fn init_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
		output_asset: Asset,
		request_type: SwapRequestType<Self::AccountId>,
		broker_fees: Beneficiaries<Self::AccountId>,
		price_limits_and_expiry: Option<PriceLimitsAndExpiry<Self::AccountId>>,
		dca_params: Option<DcaParameters>,
		origin: SwapOrigin<Self::AccountId>,
	) -> SwapRequestId {
		let request_id = SwapRequestIdCounter::<T>::mutate(|id| {
			id.saturating_accrue(1);
			*id
		});

		// Do not limit the maximum swap amount for network fee swaps.
		let net_amount = if matches!(
			request_type,
			SwapRequestType::NetworkFee | SwapRequestType::IngressEgressFee
		) {
			input_amount
		} else {
			let (swap_amount, confiscated_amount) = match MaximumSwapAmount::<T>::get(input_asset) {
				Some(max) =>
					(sp_std::cmp::min(input_amount, max), input_amount.saturating_sub(max)),
				None => (input_amount, Zero::zero()),
			};
			if !confiscated_amount.is_zero() {
				CollectedRejectedFunds::<T>::mutate(input_asset, |fund| {
					*fund = fund.saturating_add(confiscated_amount)
				});
				Self::deposit_event(Event::<T>::SwapAmountConfiscated {
					swap_request_id: request_id,
					asset: input_asset,
					total_amount: input_amount,
					confiscated_amount,
				});
			}
			swap_amount
		};

		// Restrict the number of chunks based on the minimum chunk size.
		let dca_params = dca_params.map(|mut dca_params| {
			let minimum_chunk_size = MinimumChunkSize::<T>::get(input_asset);
			if minimum_chunk_size > 0 {
				dca_params.number_of_chunks = core::cmp::min(
					max((input_amount / minimum_chunk_size) as u32, 1),
					dca_params.number_of_chunks,
				);
			}

			// There has to be a at least one chunk
			dca_params.number_of_chunks = core::cmp::max(dca_params.number_of_chunks, 1);

			dca_params
		});

		// Enforce the default oracle price protection for regular swaps
		let processed_price_limits_and_expiry = match request_type {
			SwapRequestType::Regular { .. } | SwapRequestType::RegularNoNetworkFee { .. } => {
				price_limits_and_expiry.map(|limits| PriceLimitsAndExpiry {
					// Only apply default oracle protection if no slippage is already set
					max_oracle_price_slippage: if limits.max_oracle_price_slippage.is_none() {
						Self::get_default_oracle_price_protection(input_asset, output_asset)
					} else {
						limits.max_oracle_price_slippage
					},
					..limits
				})
			},
			SwapRequestType::NetworkFee |
			SwapRequestType::IngressEgressFee |
			SwapRequestType::BrokerFee { .. } => None,
		};

		Self::deposit_event(Event::<T>::SwapRequested {
			swap_request_id: request_id,
			input_asset,
			input_amount,
			output_asset,
			request_type: request_type.clone().into_encoded::<T::AddressConverter>(),
			origin: origin.clone(),
			broker_fees: broker_fees.clone(),
			price_limits_and_expiry: processed_price_limits_and_expiry.clone(),
			dca_parameters: dca_params.clone(),
		});

		match request_type {
			SwapRequestType::NetworkFee => {
				Self::schedule_swap(
					input_asset,
					output_asset,
					net_amount,
					// No refund parameters for network fee swaps
					None,
					SwapType::NetworkFee,
					request_id,
					SWAP_DELAY_BLOCKS.into(),
				);

				SwapRequests::<T>::insert(
					request_id,
					SwapRequest {
						id: request_id,
						input_asset,
						output_asset,
						state: SwapRequestState::NetworkFee,
					},
				);
			},
			SwapRequestType::IngressEgressFee => {
				Self::schedule_swap(
					input_asset,
					output_asset,
					net_amount,
					// No refund parameters for ingress/egress fee swaps
					None,
					SwapType::IngressEgressFee,
					request_id,
					SWAP_DELAY_BLOCKS.into(),
				);

				SwapRequests::<T>::insert(
					request_id,
					SwapRequest {
						id: request_id,
						input_asset,
						output_asset,
						state: SwapRequestState::IngressEgressFee,
					},
				);
			},
			SwapRequestType::BrokerFee { account_id } => {
				Self::schedule_swap(
					input_asset,
					output_asset,
					net_amount,
					// No refund parameters for broker fee swaps
					None,
					SwapType::BrokerFee,
					request_id,
					SWAP_DELAY_BLOCKS.into(),
				);

				SwapRequests::<T>::insert(
					request_id,
					SwapRequest {
						id: request_id,
						input_asset,
						output_asset,
						state: SwapRequestState::BrokerFee { account_id },
					},
				);
			},
			SwapRequestType::Regular { ref output_action } |
			SwapRequestType::RegularNoNetworkFee { ref output_action } => {
				let mut dca_state = DcaState::new(net_amount, dca_params.clone());
				let chunk_input_amount = dca_state.calculate_next_chunk().unwrap_or_default();

				let swap_id = Self::schedule_swap(
					input_asset,
					output_asset,
					chunk_input_amount,
					processed_price_limits_and_expiry.as_ref(),
					SwapType::Swap,
					request_id,
					SWAP_DELAY_BLOCKS.into(),
				);

				dca_state.record_scheduled_chunk(swap_id, chunk_input_amount);

				if let Some(DcaParameters { chunk_interval, .. }) = dca_params {
					// This assumes that the swap delay is 2, so we will only even schedule max
					// of 2 chunks at a time.
					if chunk_interval == 1 {
						// Also schedule a second swap so we can have an chunk interval that is
						// smaller than the swap delay.
						let chunk_input_amount =
							dca_state.calculate_next_chunk().unwrap_or_default();
						if chunk_input_amount > 0 {
							let swap_id = Self::schedule_swap(
								input_asset,
								output_asset,
								chunk_input_amount,
								processed_price_limits_and_expiry.as_ref(),
								SwapType::Swap,
								request_id,
								SWAP_DELAY_BLOCKS.saturating_add(chunk_interval).into(),
							);
							dca_state.record_scheduled_chunk(swap_id, chunk_input_amount);
						}
					}
				}

				SwapRequests::<T>::insert(
					request_id,
					SwapRequest {
						id: request_id,
						input_asset,
						output_asset,
						state: SwapRequestState::UserSwap {
							output_action: output_action.clone(),
							price_limits_and_expiry: processed_price_limits_and_expiry,
							dca_state,
							network_fee_tracker: NetworkFeeTracker::new(
								Pallet::<T>::get_network_fee_for_swap(
									input_asset,
									output_asset,
									NetworkFeeType::from_swap_request_type(&request_type),
								),
							),
							broker_fees_tracker: BrokerFeesTracker::new(broker_fees),
						},
					},
				);
			},
		};

		request_id
	}

	fn inspect_swap_request(swap_request_id: SwapRequestId) -> Option<SwapExecutionProgress> {
		let swap_request = SwapRequests::<T>::get(swap_request_id)?;

		let SwapRequestState::UserSwap { dca_state, .. } = swap_request.state else {
			return None;
		};

		let scheduled_swaps = ScheduledSwaps::<T>::get();

		let input_amount_in_scheduled_swaps: AssetAmount = dca_state
			.scheduled_chunks
			.iter()
			.filter_map(|swap_id| scheduled_swaps.get(swap_id).map(|swap| swap.input_amount))
			.sum();

		Some(SwapExecutionProgress {
			remaining_input_amount: dca_state.remaining_input_amount +
				input_amount_in_scheduled_swaps,
			accumulated_output_amount: dca_state.accumulated_output_amount,
		})
	}

	fn abort_swap_request(swap_request_id: SwapRequestId) -> Option<SwapExecutionProgress> {
		let swap_progress = Self::inspect_swap_request(swap_request_id)?;

		// Cancel any scheduled swaps:
		let request = SwapRequests::<T>::take(swap_request_id)?;

		let broker_fee_swaps = if let SwapRequestState::UserSwap {
			dca_state,
			broker_fees_tracker,
			..
		} = request.state
		{
			for swap_id in dca_state.scheduled_chunks {
				Self::cancel_swap(swap_id, SwapFailureReason::AbortedFromOrigin);
			}

			// Start swaps for any accumulated broker fees
			Self::start_broker_fee_swaps_or_credit(&broker_fees_tracker, request.output_asset)
		} else {
			BTreeMap::new()
		};

		Self::deposit_event(Event::<T>::SwapRequestCompleted {
			swap_request_id,
			reason: SwapRequestCompletionReason::Aborted,
			broker_fee_swaps,
		});

		Some(swap_progress)
	}
}

// TODO JAMIE: move to impls file?
impl<T: Config> AssetConverter for Pallet<T> {
	fn calculate_input_for_desired_output_or_default_to_zero(
		input_asset: Asset,
		output_asset: Asset,
		desired_output_amount: AssetAmount,
		with_network_fee: bool,
	) -> AssetAmount {
		// Approximate one sided slippage adjustments for oracle prices
		const ORACLE_SLIPPAGE: BasisPoints = 40;
		const ORACLE_SLIPPAGE_STABLE: BasisPoints = 3;

		if desired_output_amount.is_zero() {
			return 0;
		}

		// Ignoring the minimum network fee because this function is only used for fees and
		// gas (no minimum).
		let network_fee = Pallet::<T>::get_network_fee_rate_for_swap(
			input_asset,
			output_asset,
			if with_network_fee { NetworkFeeType::NoMinimum } else { NetworkFeeType::None },
		);

		let required_input = if input_asset == output_asset {
			desired_output_amount
		} else {
			// Get the price of both assets using oracles or simulated swap, both with fallback
			// to hard coded prices
			let get_usd_price = |asset, side: Side| -> Price {
				if asset == Asset::Flip || asset == Asset::Dot {
					let asset_price =
						Self::estimate_usdc_price_using_simulated_swap_or_fallback(asset, side); // USDC / Asset
					let usdc_price = T::PriceFeedApi::get_price(Asset::Usdc) // USD / USDC
						// Ignore staleness because it will always be less stale
						// than hard-coded prices.
						.map(|price_data| price_data.price)
						.unwrap_or_else(Price::one);
					asset_price.multiply_by(usdc_price) // USD / Asset
				} else {
					// Using stale prices here is fine as its just for fees/gas
					T::PriceFeedApi::get_price(asset)
						.map(|price_data| {
							// Apply a hard coded slippage in the correct direction
							let slippage_bps = if asset == Asset::Usdc {
								0
							} else if asset.is_usd_stablecoin() {
								ORACLE_SLIPPAGE_STABLE
							} else {
								ORACLE_SLIPPAGE
							};
							price_data.price.adjust_by_bps(slippage_bps, side == Side::Buy)
						})
						.unwrap_or_else(|| utilities::hard_coded_price_for_asset(asset))
				}
			};
			let output_price = get_usd_price(output_asset, Side::Buy); // USD / output_asset
			let input_price = get_usd_price(input_asset, Side::Sell); // USD / input_asset
			if input_price.is_zero() || output_price.is_zero() {
				log_or_panic!(
					"Estimated Price for input or output asset is zero: {input_asset:?} = {input_price:?}, {output_asset:?} = {output_price:?}"
				);
				return 0;
			}
			// (USD / output_asset) / (USD / input_asset) = input_asset / output_asset
			let relative_price = output_price.divide_by(input_price);

			// Finally calculate the required input amount
			relative_price
				.output_amount_ceil(desired_output_amount)
				.saturated_into::<AssetAmount>()
		};

		// Adjust for network fee
		if network_fee.is_one() {
			0
		} else {
			FixedU64::from_rational(
				ONE_AS_BASIS_POINTS as u128,
				ONE_AS_BASIS_POINTS as u128 - network_fee * (ONE_AS_BASIS_POINTS as u128),
			)
			.saturating_mul_int(required_input)
		}
	}
}
