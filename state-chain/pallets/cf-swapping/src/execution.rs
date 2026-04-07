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

use cf_primitives::{basis_points::SignedHundredthBasisPoints, DcaParameters};
use cf_traits::{
	lending::LendingSystemApi, EgressApi, FundAccount, PoolPriceProvider, ScheduledEgressDetails,
	SwapExecutionProgress,
};
use core::cmp::max;
use sp_runtime::{FixedPointNumber, FixedU64, SaturatedConversion};

use super::{
	pallet::{Config, Event, Pallet, SwapRequests},
	*,
};
impl<T: Config> Pallet<T> {
	pub fn get_scheduled_swap_legs(base_asset: Asset) -> Vec<(SwapLegInfo, BlockNumberFor<T>)> {
		if !T::SafeMode::get().swaps_enabled {
			return vec![];
		}

		// Get all of the swaps that are scheduled to swap from or to the given asset.
		let swaps: Vec<_> = ScheduledSwaps::<T>::get()
			.values()
			.filter_map(|swap| {
				if swap.from == base_asset || swap.to == base_asset {
					// We ignore price protection for the simulation
					Some(swap.clone().without_price_protection())
				} else {
					None
				}
			})
			.collect();

		// Run the normal swap execution logic on these swaps to get the amounts and
		// intermediate swap outcomes.
		let BatchExecutionOutcomes { successful_swaps, failed_swaps } =
			Self::execute_batch(swaps.clone());

		// For all failed swaps, we can use price pool fallback to estimate the amounts
		let failed_swaps: Vec<SwapState<T, Stage2>> = if !failed_swaps.is_empty() {
			let swaps = failed_swaps.into_iter().map(|(swap, _)| SwapState::new(swap)).collect();

			// Manually execute the fist step of the swap logic to get the input amount
			// after fees
			Self::take_network_fees(swaps)
				.into_iter()
				.filter_map(|state| {
					let intermediate_amount = if state.input_asset() == STABLE_ASSET ||
						state.output_asset() == STABLE_ASSET
					{
						// Single leg swap, so no intermediate amount.
						None
					} else {
						// Pool price fallback. If pool does not exist, the swap will be
						// filtered out.
						let sell_price =
							T::PoolPriceApi::pool_price(state.input_asset(), STABLE_ASSET)
								.ok()
								.map(|price| price.sell)?;

						Some(
							sell_price
								.output_amount_ceil(state.stage.input_amount_after_fees)
								.saturated_into(),
						)
					};

					Some(
						state.with_intermediate(
							intermediate_amount
								.map(|amount| AssetAndAmount { asset: STABLE_ASSET, amount }),
						),
					)
				})
				.collect()
		} else {
			// No failed swaps
			vec![]
		};

		successful_swaps
			.into_iter()
			// We only need the swaps at the stage after the first leg is complete. So we
			// strip away the unused part of the state for the fully executed swaps.
			.map(SwapState::from)
			.chain(failed_swaps)
			.filter_map(|state| {
				let swap_request = SwapRequests::<T>::get(state.swap_request_id())
					.expect("Swap request should exist");
				let dca_state = match swap_request.state {
					SwapRequestState::UserSwap { dca_state, .. } => Some(dca_state),
					_ => None,
				};
				let remaining_chunks =
					dca_state.as_ref().map(|dca| dca.remaining_chunks).unwrap_or(0);
				let chunk_interval =
					dca_state.map(|dca| dca.chunk_interval).unwrap_or(SWAP_DELAY_BLOCKS);

				if state.input_asset() != STABLE_ASSET && state.input_asset() == base_asset {
					Some((
						SwapLegInfo {
							swap_id: state.swap_id(),
							swap_request_id: state.swap_request_id(),
							base_asset,
							// All swaps from `base_asset` have to go through the stable asset:
							quote_asset: STABLE_ASSET,
							side: Side::Sell,
							amount: state.stage.input_amount_after_fees,
							source_asset: None,
							source_amount: None,
							remaining_chunks,
							chunk_interval,
						},
						state.execute_at(),
					))
				} else if state.output_asset() != STABLE_ASSET && state.output_asset() == base_asset
				{
					// In case the swap is "simulated", the amount is just an estimate,
					// so we additionally include `source_asset` and `source_amount`:
					let (source_asset, source_amount) = if state.input_asset() != STABLE_ASSET {
						(Some(state.input_asset()), Some(state.stage.input_amount_after_fees))
					} else {
						(None, None)
					};

					Some((
						SwapLegInfo {
							swap_id: state.swap_id(),
							swap_request_id: state.swap_request_id(),
							base_asset,
							// All swaps to `base_asset` have to go through the stable asset:
							quote_asset: STABLE_ASSET,
							side: Side::Buy,
							// If the intermediate is None, then it means the swap input was
							// already in the stable asset.
							amount: state
								.stage
								.intermediate
								.as_ref()
								.map(|intermediate| intermediate.amount)
								.unwrap_or(state.input_amount_before_fees()),
							source_asset,
							source_amount,
							remaining_chunks,
							chunk_interval,
						},
						state.execute_at(),
					))
				} else {
					None
				}
			})
			.collect()
	}

	pub(crate) fn trigger_withdrawal(
		account_id: &T::AccountId,
		asset: Asset,
		destination_address: ForeignChainAddress,
	) -> DispatchResult {
		let earned_fees = T::BalanceApi::get_balance(account_id, asset);
		ensure!(earned_fees != 0, Error::<T>::NoFundsAvailable);
		T::BalanceApi::try_debit_account(account_id, asset, earned_fees)?;

		let ScheduledEgressDetails { egress_id, egress_amount, fee_withheld } =
			T::EgressHandler::schedule_egress(
				asset,
				earned_fees,
				destination_address.clone(),
				None,
			)
			.map_err(Into::into)?;

		Self::deposit_event(Event::<T>::WithdrawalRequested {
			account_id: account_id.clone(),
			egress_amount,
			egress_asset: asset,
			egress_fee: fee_withheld,
			destination_address: T::AddressConverter::to_encoded_address(destination_address),
			egress_id,
		});

		Ok(())
	}

	pub(crate) fn take_network_fees(swaps: Vec<SwapState<T, ()>>) -> Vec<SwapState<T, Stage1>> {
		let mut total_network_fee_taken = BTreeMap::<Asset, AssetAmount>::new();
		let swaps_after_network_fees: Vec<SwapState<T, Stage1>> = swaps
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
										false, // is_internal_swap
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
						total_network_fee_taken
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

		// Accrue the total network fees taken in storage
		for (asset, total) in total_network_fee_taken {
			CollectedNetworkFee::<T>::mutate(asset, |collected_fee| {
				collected_fee.saturating_accrue(total)
			})
		}

		swaps_after_network_fees
	}

	pub(crate) fn take_broker_fees(swaps: Vec<SwapState<T, Stage3>>) -> Vec<SwapState<T, Stage4>> {
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

	pub(crate) fn start_broker_fee_swaps_or_credit(
		fee_tracker: &BrokerFeesTracker<T::AccountId>,
		fee_asset: Asset,
	) -> BTreeMap<T::AccountId, SwapRequestId> {
		if fee_asset == Asset::Usdc {
			// No need to swap if the fee asset is already in usdc, just credit directly.
			for (Beneficiary { account, .. }, amount) in &fee_tracker.fee_and_accumulated {
				T::BalanceApi::credit_account(account, fee_asset, *amount)
			}
			return BTreeMap::new();
		}

		// For each beneficiary start a swap to convert the fee into usdc.
		fee_tracker
			.fee_and_accumulated
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

	/// Enforce price protections. Must be called after the final output has been set.
	pub(crate) fn check_swap_price_violation(
		swap: &SwapState<T, Stage4>,
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

		// Calculate the slippage from oracle prices for both legs of the swap (without being
		// affected by fees).
		let (first_leg_delta, second_leg_delta) = if let Some(intermediate) =
			&swap.stage.intermediate
		{
			(
				Self::get_delta_from_oracle_price(
					AssetAndAmount::new(swap.input_asset(), swap.stage.input_amount_after_fees),
					*intermediate,
				)?,
				Self::get_delta_from_oracle_price(
					*intermediate,
					AssetAndAmount::new(swap.output_asset(), swap.stage.output_amount_before_fees),
				)?,
			)
		} else {
			// No intermediate asset, so must be a single leg swap.
			(
				Self::get_delta_from_oracle_price(
					AssetAndAmount::new(swap.input_asset(), swap.stage.input_amount_after_fees),
					AssetAndAmount::new(swap.output_asset(), swap.stage.output_amount_before_fees),
				)?,
				None,
			)
		};

		// Sum the deltas or just use a single leg delta if the other leg doesn't have
		// an oracle price.
		let total_delta = match (first_leg_delta, second_leg_delta) {
			(Some(first_leg), Some(second_leg)) => Some(first_leg.saturating_add(&second_leg)),
			(Some(delta), None) | (None, Some(delta)) => Some(delta),
			_ => None,
		};

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
		swaps: Vec<Swap<T>>,
	) -> Result<Vec<SwapState<T, Stage5>>, BatchExecutionError<T>> {
		// Bundle each swap with a fresh swap state
		let swaps: Vec<_> = swaps.into_iter().map(SwapState::new).collect();
		// Take the network fee
		let swaps = Self::take_network_fees(swaps);
		// Run the first leg of the swaps
		let swaps = Self::do_group_and_swap(swaps)?;
		// Run the second leg of the swaps
		let swaps = Self::do_group_and_swap(swaps)?;
		// Take the broker fees
		let swaps = Self::take_broker_fees(swaps);

		// Successfully executed without hitting price impact limit.
		// Now check for price violations (oracle and minimum price).
		let mut non_violating_swaps = vec![];
		let mut violating_swaps = vec![];
		swaps.into_iter().for_each(|swap| match swap.check_for_price_violation() {
			Ok(swap) => {
				non_violating_swaps.push(swap);
			},
			Err((state, reason)) => {
				violating_swaps.push((state.into_swap(), reason));
			},
		});

		if violating_swaps.is_empty() {
			Ok(non_violating_swaps)
		} else {
			Err(BatchExecutionError::PriceViolation {
				violating_swaps,
				non_violating_swaps: non_violating_swaps
					.into_iter()
					.map(|state| state.into_swap())
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
	) -> Result<SwapState<T, Stage5>, BatchExecutionError<T>> {
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
				Self::try_execute_without_violations(vec![swap]).and_then(|swaps| {
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
	pub(crate) fn execute_batch(mut swaps_to_execute: Vec<Swap<T>>) -> BatchExecutionOutcomes<T> {
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
							SwapLeg::FromStable
						} else {
							SwapLeg::ToStable
						},
						amount,
					});

					// Find the largest swap from the failing pool/direction and remove it
					// so we can try the remaining swaps again. We should always be able to
					// find a swap to remove, but if we can't for some reason, abort.
					if let Some(removed_swap) = utilities::split_off_highest_impact_swap(
						&mut swaps_to_execute,
						failed_swap_group,
					) {
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
					swaps_to_execute = non_violating_swaps;
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
				.into_iter()
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
				..
			} => {
				let Some(ExpiryBehaviour::RefundIfExpires {
					refund_address,
					refund_ccm_metadata,
					..
				}) = price_limits_and_expiry.as_ref().map(|p| &p.expiry_behaviour)
				else {
					log_or_panic!("Trying to refund swap request {swap_request_id}, but missing refund parameters");
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

				let total_input_remaining =
					swap.input_amount + dca_state.remaining_input_amount + canceled_swaps_amount;

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

	pub(crate) fn process_swap_outcome(swap: SwapState<T, Stage5>) {
		let swap_request_id = swap.swap_request_id();

		let Some(mut request) = SwapRequests::<T>::take(swap_request_id) else {
			log_or_panic!("Swap request {swap_request_id} not found");
			return;
		};

		Self::deposit_event(Event::<T>::SwapExecuted {
			swap_request_id,
			swap_id: swap.swap_id(),
			input: AssetAndAmount::new(swap.input_asset(), swap.stage.input_amount_after_fees),
			network_fee: AssetAndAmount::new(swap.input_asset(), swap.stage.network_fee_taken),
			broker_fee: AssetAndAmount::new(swap.output_asset(), swap.stage.broker_fee_taken),
			output: AssetAndAmount::new(swap.output_asset(), swap.stage.output_amount_after_fees),
			intermediate: swap.stage.intermediate,
			oracle_delta: swap.stage.oracle_delta,
			oracle_delta_ex_fees: swap.stage.oracle_delta_ex_fees,
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
					dca_state.record_chunk_completion(
						swap.swap_id(),
						swap.stage.output_amount_after_fees,
					);

					(false, Default::default())
				} else {
					debug_assert!(dca_state.remaining_input_amount == 0);

					dca_state.record_chunk_completion(
						swap.swap_id(),
						swap.stage.output_amount_after_fees,
					);

					// This may or may not be the last chunk (some may already be scheduled)
					if dca_state.scheduled_chunks.is_empty() {
						match output_action {
							SwapOutputAction::Egress { ccm_deposit_metadata, output_address } => {
								Self::egress_for_swap(
									swap_request_id,
									dca_state.accumulated_output_amount,
									swap.output_asset(),
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
									if swap.stage.output_amount_after_fees <
										*flip_to_subtract_from_swap_output
									{
										// In the rare event that this occurs we will track the
										// deficit and offset it against the next burn
										FlipToBurn::<T>::mutate(|total| {
											total.saturating_reduce(
												flip_to_subtract_from_swap_output
													.saturating_sub(
														swap.stage.output_amount_after_fees,
													)
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
											swap.stage
												.output_amount_after_fees
												.saturating_sub(*flip_to_subtract_from_swap_output)
												.into(),
											FundingSource::Swap { swap_request_id },
										);
										FlipToBeSentToGateway::<T>::mutate(|total| {
											total.saturating_accrue(
												swap.stage.output_amount_after_fees,
											);
										});
									}
								} else {
									log_or_panic!("Encountered transfer to gateway swap for asset that isn't Flip: {swap_request_id:?}");
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
				if swap.output_asset() == Asset::Flip {
					FlipToBurn::<T>::mutate(|total| {
						total.saturating_accrue(
							swap.stage.output_amount_after_fees.try_into().unwrap_or(i128::MAX),
						);
					});
				} else {
					log_or_panic!(
						"NetworkFee burning should not be in asset: {:?}",
						swap.output_asset()
					);
				}
				(true, Default::default())
			},
			SwapRequestState::IngressEgressFee => {
				if swap.output_asset() == ForeignChain::from(swap.output_asset()).gas_asset() {
					T::IngressEgressFeeHandler::accrue_withheld_fee(
						swap.output_asset(),
						swap.stage.output_amount_after_fees,
					);
				} else {
					log_or_panic!(
						"IngressEgressFee swap should not be to non-gas asset: {:?}",
						swap.output_asset()
					);
				}

				(true, Default::default())
			},
			SwapRequestState::BrokerFee { account_id } => {
				T::BalanceApi::credit_account(
					account_id,
					swap.output_asset(),
					swap.stage.output_amount_after_fees,
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

	// Groups the swaps that are swapping from->to the same assets and does a large single swap
	// for each group. Then splits the output among the individual swaps. Processed and
	// unprocessed swaps are returned.
	pub(crate) fn do_group_and_swap<CurrentState: GroupSwapState<T> + Debug>(
		swaps: Vec<CurrentState>,
	) -> Result<Vec<CurrentState::OutputState>, BatchExecutionError<T>> {
		let mut outcome_swaps = Vec::new();
		let mut swap_groups = BTreeMap::<SwapGroupPair, Vec<CurrentState>>::new();

		// First split the swaps into groups for this leg
		for swap in swaps {
			if let Some(swap_group_pair) = swap.swap_group() {
				swap_groups.entry(swap_group_pair).or_default().push(swap);
			} else {
				// If the swap doesn't need to be swapped because its already in the correct
				// asset, then we can just update the swap state and leave it be.
				outcome_swaps.push(swap.update_no_swap());
			}
		}

		for (swap_group_pair, swaps) in swap_groups {
			outcome_swaps.extend(
				Self::execute_group_of_swaps(swaps, swap_group_pair.clone()).map_err(
					|(failed_swaps, amount)| BatchExecutionError::SwapLegFailed {
						from_asset: swap_group_pair.from,
						to_asset: swap_group_pair.to,
						amount,
						failed_swap_group: failed_swaps
							.into_iter()
							.map(|swap| swap.failed_swap())
							.collect::<Vec<SwapState<T, StageFailed>>>(),
					},
				)?,
			);
		}
		Ok(outcome_swaps)
	}

	/// Bundle the given swaps and do a single swap of a given direction
	pub(crate) fn execute_group_of_swaps<CurrentState: GroupSwapState<T> + Debug>(
		swaps: Vec<CurrentState>,
		swap_group_pair: SwapGroupPair,
	) -> Result<Vec<CurrentState::OutputState>, (Vec<CurrentState>, AssetAmount)> {
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
					if index == number_of_swaps - 1 {
						// Give the dust to the last swap to ensure all output is assigned
						let swap_output = bundle_output.saturating_sub(total_assigned_output);
						swap.update_swap_result(swap_output)
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
						swap.update_swap_result(swap_output)
					}
				})
				.collect())
		} else {
			Err((swaps, bundle_input))
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

	pub fn estimate_usdc_price_using_simulated_swap_or_fallback(asset: Asset, side: Side) -> Price {
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

	pub(crate) fn egress_for_swap(
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

	pub fn assemble_and_validate_broker_fees(
		broker_id: T::AccountId,
		broker_commission: BasisPoints,
		affiliate_fees: Affiliates<T::AccountId>,
	) -> Result<Beneficiaries<T::AccountId>, DispatchError> {
		let beneficiaries = [Beneficiary { account: broker_id, bps: broker_commission }]
			.into_iter()
			.chain(affiliate_fees.iter().cloned())
			.collect::<Vec<_>>()
			.try_into()
			.expect(
				"We are pushing affiliates + 1 which is exactly the maximum Beneficiaries size",
			);
		Pallet::<T>::validate_broker_fees(&beneficiaries)?;
		Ok(beneficiaries)
	}

	/// Gets the network fee rate and minimum in usdc terms for a swap between the given input
	/// and output assets, taking into account whether it's an internal swap or not.
	pub(crate) fn get_network_fee(
		input_asset: Asset,
		output_asset: Asset,
		is_internal_swap: bool,
	) -> FeeRateAndMinimum {
		let (input_asset_fee, output_asset_fee, usdc_minimum) = if is_internal_swap {
			let default_fee = InternalSwapNetworkFee::<T>::get();
			(
				InternalSwapNetworkFeeForAsset::<T>::get(input_asset).unwrap_or(default_fee.rate),
				InternalSwapNetworkFeeForAsset::<T>::get(output_asset).unwrap_or(default_fee.rate),
				default_fee.minimum,
			)
		} else {
			let default_fee = NetworkFee::<T>::get();
			(
				NetworkFeeForAsset::<T>::get(input_asset).unwrap_or(default_fee.rate),
				NetworkFeeForAsset::<T>::get(output_asset).unwrap_or(default_fee.rate),
				default_fee.minimum,
			)
		};

		FeeRateAndMinimum { rate: input_asset_fee.max(output_asset_fee), minimum: usdc_minimum }
	}

	pub fn get_network_fee_rate_for_swap(
		input_asset: Asset,
		output_asset: Asset,
		is_internal_swap: bool,
	) -> Permill {
		Self::get_network_fee(input_asset, output_asset, is_internal_swap).rate
	}

	pub fn get_network_fee_for_swap(
		input_asset: Asset,
		output_asset: Asset,
		is_internal_swap: bool,
		with_minimum: bool,
	) -> FeeRateAndMinimum {
		// Find the correct fee values in USDC
		let FeeRateAndMinimum { rate, minimum: usdc_minimum } =
			Self::get_network_fee(input_asset, output_asset, is_internal_swap);

		// Convert the minimum amount to the input asset
		let minimum = if with_minimum {
			Pallet::<T>::calculate_input_for_desired_output_or_default_to_zero(
				input_asset,
				Asset::Usdc,
				usdc_minimum,
				false, // no network fee
				false, // not internal
			)
		} else {
			0
		};

		FeeRateAndMinimum { rate, minimum }
	}

	/// Returns the configured default oracle price slippage protection for a single pool leg.
	/// Returns `None` if the asset has no oracle price feed or no valid pool pair.
	pub fn default_oracle_lpp_for_asset(asset: Asset) -> Option<BasisPoints> {
		T::PriceFeedApi::get_price(asset)?;
		Some(DefaultOraclePriceSlippageProtection::<T>::get(AssetPair::new(asset, STABLE_ASSET)?))
	}

	/// Returns the default price protection to apply to a one or two-leg swap.
	///
	/// Returns `None` if no oracle price is available for any leg of the swap (no
	/// meaningful protection can be applied). For two-leg swaps where only one leg
	/// has an oracle, the other leg contributes zero to the total limit so that
	/// the oracle-priced leg is still protected.
	pub(crate) fn get_default_oracle_price_protection(
		input_asset: Asset,
		output_asset: Asset,
	) -> Option<BasisPoints> {
		match (input_asset, output_asset) {
			// Swaps to/from the stable asset use a single pool, so only one
			// leg's slippage applies.
			(STABLE_ASSET, asset) | (asset, STABLE_ASSET) =>
				Self::default_oracle_lpp_for_asset(asset),
			// Non-stable swaps go through two pools. At least one leg must have
			// oracle data for the default to be meaningful.
			(input_asset, output_asset) => match (
				Self::default_oracle_lpp_for_asset(input_asset),
				Self::default_oracle_lpp_for_asset(output_asset),
			) {
				(Some(input_lpp), Some(output_lpp)) => Some(input_lpp.saturating_add(output_lpp)),
				(Some(lpp), None) | (None, Some(lpp)) => Some(lpp),
				(None, None) => None,
			},
		}
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

				// Choose correct network fee for the swap
				let network_fee_rate_and_minimum =
					if matches!(request_type, SwapRequestType::Regular { output_action: _ }) {
						Pallet::<T>::get_network_fee_for_swap(
							input_asset,
							output_asset,
							// TODO: see if we want to treat lending swaps as internal for
							// the purposes of determining network fee?
							matches!(output_action, SwapOutputAction::CreditOnChain { .. }),
							true, // with minimum
						)
					} else {
						// No network fee for RegularNoNetworkFee
						FeeRateAndMinimum { rate: Permill::zero(), minimum: 0 }
					};

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
								network_fee_rate_and_minimum,
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

impl<T: Config> AssetConverter for Pallet<T> {
	fn calculate_input_for_desired_output_or_default_to_zero(
		input_asset: Asset,
		output_asset: Asset,
		desired_output_amount: AssetAmount,
		with_network_fee: bool,
		is_internal_swap: bool,
	) -> AssetAmount {
		// Approximate one sided slippage adjustments for oracle prices
		const ORACLE_SLIPPAGE: BasisPoints = 40;
		const ORACLE_SLIPPAGE_STABLE: BasisPoints = 3;

		if desired_output_amount.is_zero() {
			return 0;
		}

		let network_fee = if with_network_fee {
			// Ignoring the minimum network fee because this function is only used for fees and
			// gas (no minimum).
			Pallet::<T>::get_network_fee_rate_for_swap(input_asset, output_asset, is_internal_swap)
		} else {
			Permill::zero()
		};

		let required_input = if input_asset == output_asset {
			desired_output_amount
		} else {
			// Get the price of both assets using oracles or simulated swap, both with fallback
			// to hard coded prices
			let get_usd_price = |asset, side: Side| -> Price {
				if asset == Asset::Flip || asset == Asset::Dot {
					let asset_price =
						Self::estimate_usdc_price_using_simulated_swap_or_fallback(asset, side); // USDC / Asset
					let usdc_price = T::PriceFeedApi::get_price(STABLE_ASSET) // USD / USDC
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
							let slippage_bps = if asset == STABLE_ASSET {
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
