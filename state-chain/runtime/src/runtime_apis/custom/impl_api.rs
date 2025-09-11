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

use super::decl_api::*;
use sp_api::impl_runtime_apis;

use crate::{chainflip::Offence, Runtime, RuntimeSafeMode};
use pallet_cf_elections::electoral_systems::oracle_price::chainlink::OraclePrice;

use cf_amm::{
	common::{PoolPairsMap, Side},
	math::{Amount, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	self, address::EncodedAddress, assets::any::AssetMap, eth::Address as EthereumAddress,
	sol::SolInstructionRpc, CcmChannelMetadataUnchecked, Chain, ChainCrypto,
	ChannelRefundParametersUncheckedEncoded, ForeignChainAddress, VaultSwapExtraParametersEncoded,
	VaultSwapInputEncoded,
};
use cf_primitives::{
	AccountRole, Affiliates, Asset, AssetAmount, BasisPoints, BlockNumber, BroadcastId, ChannelId,
	DcaParameters, EpochIndex, FlipBalance, ForeignChain, GasAmount, NetworkEnvironment, SemVer,
};
use cf_traits::SwapLimits;
use codec::{Decode, Encode};
use core::{ops::Range, str};
use frame_support::sp_runtime::AccountId32;
use pallet_cf_elections::electoral_systems::oracle_price::price::PriceAsset;
use pallet_cf_governance::GovCallHash;
pub use pallet_cf_ingress_egress::ChannelAction;
pub use pallet_cf_lending_pools::BoostPoolDetails;
use pallet_cf_pools::{
	AskBidMap, PoolInfo, PoolLiquidity, PoolOrderbook, PoolOrders, PoolPriceV1, PoolPriceV2,
	UnidirectionalPoolDepth,
};
use pallet_cf_swapping::{AffiliateDetails, FeeRateAndMinimum, SwapLegInfo};
use pallet_cf_trading_strategy::TradingStrategy;
use pallet_cf_validator::OperatorSettings;
use pallet_cf_witnesser::CallHash;
use scale_info::{prelude::string::String, TypeInfo};
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;
use sp_runtime::{DispatchError, Permill};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec::Vec,
};

impl_runtime_apis! {
	impl runtime_apis::CustomRuntimeApi<Block> for Runtime {
		fn cf_is_auction_phase() -> bool {
			Validator::is_auction_phase()
		}
		fn cf_eth_flip_token_address() -> EthereumAddress {
			Environment::supported_eth_assets(cf_primitives::chains::assets::eth::Asset::Flip).expect("FLIP token address should exist")
		}
		fn cf_eth_state_chain_gateway_address() -> EthereumAddress {
			Environment::state_chain_gateway_address()
		}
		fn cf_eth_key_manager_address() -> EthereumAddress {
			Environment::key_manager_address()
		}
		fn cf_eth_chain_id() -> u64 {
			Environment::ethereum_chain_id()
		}
		fn cf_eth_vault() -> ([u8; 33], BlockNumber) {
			let epoch_index = Self::cf_current_epoch();
			// We should always have a Vault for the current epoch, but in case we do
			// not, just return an empty Vault.
			(EvmThresholdSigner::keys(epoch_index).unwrap_or_default().to_pubkey_compressed(), EthereumVault::vault_start_block_numbers(epoch_index).unwrap().unique_saturated_into())
		}
		fn cf_auction_parameters() -> (u32, u32) {
			let auction_params = Validator::auction_parameters();
			(auction_params.min_size, auction_params.max_size)
		}
		fn cf_min_funding() -> u128 {
			MinimumFunding::<Runtime>::get().unique_saturated_into()
		}
		fn cf_current_epoch() -> u32 {
			Validator::current_epoch()
		}
		fn cf_current_compatibility_version() -> SemVer {
			Environment::current_release_version()
		}
		fn cf_epoch_duration() -> u32 {
			Validator::epoch_duration()
		}
		fn cf_current_epoch_started_at() -> u32 {
			Validator::current_epoch_started_at()
		}
		fn cf_authority_emission_per_block() -> u128 {
			Emissions::current_authority_emission_per_block()
		}
		fn cf_backup_emission_per_block() -> u128 {
			0 // Backups don't exist any more.
		}
		fn cf_flip_supply() -> (u128, u128) {
			(Flip::total_issuance(), Flip::offchain_funds())
		}
		fn cf_accounts() -> Vec<(AccountId, Vec<u8>)> {
			let mut vanity_names = AccountRoles::vanity_names();
			frame_system::Account::<Runtime>::iter_keys()
				.map(|account_id| {
					let vanity_name = vanity_names.remove(&account_id).unwrap_or_default().into();
					(account_id, vanity_name)
				})
				.collect()
		}
		fn cf_free_balances(account_id: AccountId) -> AssetMap<AssetAmount> {
			LiquidityPools::sweep(&account_id).unwrap();
			AssetBalances::free_balances(&account_id)
		}
		fn cf_lp_total_balances(account_id: AccountId) -> AssetMap<AssetAmount> {
			LiquidityPools::sweep(&account_id).unwrap();
			let free_balances = AssetBalances::free_balances(&account_id);
			let open_order_balances = LiquidityPools::open_order_balances(&account_id);

			let boost_pools_balances = AssetMap::from_fn(|asset| {
				LendingPools::boost_pool_account_balance(&account_id, asset)
			});

			free_balances.saturating_add(open_order_balances).saturating_add(boost_pools_balances)
		}
		fn cf_account_flip_balance(account_id: &AccountId) -> u128 {
			pallet_cf_flip::Account::<Runtime>::get(account_id).total()
		}
		fn cf_common_account_info(
			account_id: &AccountId,
		) -> RpcAccountInfoCommonItems<FlipBalance> {
			LiquidityPools::sweep(account_id).unwrap();
			let flip_account = pallet_cf_flip::Account::<Runtime>::get(account_id);
			let upcoming_delegation_status = pallet_cf_validator::DelegationChoice::<Runtime>::get(account_id)
				.map(|(operator, max_bid)| DelegationInfo { operator, bid: core::cmp::min(flip_account.total(), max_bid) });
			let current_delegation_status = pallet_cf_validator::DelegationSnapshots::<Runtime>::iter_prefix(Validator::current_epoch())
				.find_map(|(operator, snapshot)| snapshot.delegators.get(account_id).map(|&bid|
					DelegationInfo { operator, bid }
				));

			RpcAccountInfoCommonItems {
				flip_balance: flip_account.total(),
				asset_balances: AssetBalances::free_balances(account_id),
				bond: flip_account.bond(),
				estimated_redeemable_balance: pallet_cf_funding::Redemption::<Runtime>::for_rpc(
					account_id,
				).map(|redemption| redemption.redeem_amount).unwrap_or_default(),
				bound_redeem_address: pallet_cf_funding::BoundRedeemAddress::<Runtime>::get(account_id),
				restricted_balances: pallet_cf_funding::RestrictedBalances::<Runtime>::get(account_id),
				current_delegation_status,
				upcoming_delegation_status,
			}
		}
		fn cf_validator_info(account_id: &AccountId) -> ValidatorInfo {
			let key_holder_epochs = pallet_cf_validator::HistoricalActiveEpochs::<Runtime>::get(account_id);
			let is_qualified = <<Runtime as pallet_cf_validator::Config>::KeygenQualification as QualifyNode<_>>::is_qualified(account_id);
			let is_current_authority = pallet_cf_validator::CurrentAuthorities::<Runtime>::get().contains(account_id);
			let is_bidding = Validator::is_bidding(account_id);
			let bound_redeem_address = pallet_cf_funding::BoundRedeemAddress::<Runtime>::get(account_id);
			let apy_bp = calculate_account_apy(account_id);
			let reputation_info = pallet_cf_reputation::Reputations::<Runtime>::get(account_id);
			let account_info = pallet_cf_flip::Account::<Runtime>::get(account_id);
			let restricted_balances = pallet_cf_funding::RestrictedBalances::<Runtime>::get(account_id);
			let estimated_redeemable_balance = pallet_cf_funding::Redemption::<Runtime>::for_rpc(
				account_id,
			).map(|redemption| redemption.redeem_amount).unwrap_or_default();
			ValidatorInfo {
				balance: account_info.total(),
				bond: account_info.bond(),
				last_heartbeat: pallet_cf_reputation::LastHeartbeat::<Runtime>::get(account_id).unwrap_or(0),
				reputation_points: reputation_info.reputation_points,
				keyholder_epochs: key_holder_epochs,
				is_current_authority,
				is_current_backup: false,
				is_qualified: is_bidding && is_qualified,
				is_online: HeartbeatQualification::<Runtime>::is_qualified(account_id),
				is_bidding,
				bound_redeem_address,
				apy_bp,
				restricted_balances,
				estimated_redeemable_balance,
				operator: pallet_cf_validator::OperatorChoice::<Runtime>::get(account_id),
			}
		}

		fn cf_operator_info(account_id: &AccountId) -> OperatorInfo<FlipBalance> {
			let settings= pallet_cf_validator::OperatorSettingsLookup::<Runtime>::get(account_id).unwrap_or_default();
			let exceptions = pallet_cf_validator::Exceptions::<Runtime>::get(account_id).into_iter().collect();
			let (allowed, blocked) = match &settings.delegation_acceptance {
				DelegationAcceptance::Allow => (Default::default(), exceptions),
				DelegationAcceptance::Deny => (exceptions, Default::default()),
			};
			OperatorInfo {
				managed_validators: pallet_cf_validator::Pallet::<Runtime>::get_all_associations_by_operator(
					account_id,
					AssociationToOperator::Validator,
					|account_id, _| pallet_cf_flip::Pallet::<Runtime>::balance(account_id)
				),
				settings,
				allowed,
				blocked,
				delegators: pallet_cf_validator::Pallet::<Runtime>::get_all_associations_by_operator(
					account_id,
					AssociationToOperator::Delegator,
					|account_id, max_bid| pallet_cf_flip::Pallet::<Runtime>::balance(account_id).min(max_bid.unwrap_or(u128::MAX))
				),
				active_delegation: pallet_cf_validator::DelegationSnapshots::<Runtime>::get(
					Validator::current_epoch(),
					account_id,
				),
			}
		}

		fn cf_penalties() -> Vec<(Offence, RuntimeApiPenalty)> {
			pallet_cf_reputation::Penalties::<Runtime>::iter_keys()
				.map(|offence| {
					let penalty = pallet_cf_reputation::Penalties::<Runtime>::get(offence);
					(offence, RuntimeApiPenalty {
						reputation_points: penalty.reputation,
						suspension_duration_blocks: penalty.suspension
					})
				})
				.collect()
		}
		fn cf_suspensions() -> Vec<(Offence, Vec<(u32, AccountId)>)> {
			pallet_cf_reputation::Suspensions::<Runtime>::iter_keys()
				.map(|offence| {
					let suspension = pallet_cf_reputation::Suspensions::<Runtime>::get(offence);
					(offence, suspension.into())
				})
				.collect()
		}
		fn cf_generate_gov_key_call_hash(
			call: Vec<u8>,
		) -> GovCallHash {
			Governance::compute_gov_key_call_hash::<_>(call).0
		}

		fn cf_auction_state() -> AuctionState {
			let auction_params = Validator::auction_parameters();
			AuctionState {
				epoch_duration: Validator::epoch_duration(),
				current_epoch_started_at: Validator::current_epoch_started_at(),
				redemption_period_as_percentage: Validator::redemption_period_as_percentage().deconstruct(),
				min_funding: MinimumFunding::<Runtime>::get().unique_saturated_into(),
				min_bid: pallet_cf_validator::MinimumAuctionBid::<Runtime>::get().unique_saturated_into(),
				auction_size_range: (auction_params.min_size, auction_params.max_size),
				min_active_bid: Validator::resolve_auction_iteratively()
				.ok()
				.map(|(auction_outcome, _)| auction_outcome.bond)
			}
		}

		fn cf_pool_price(
			from: Asset,
			to: Asset,
		) -> Option<PoolPriceV1> {
			LiquidityPools::current_price(from, to)
		}

		fn cf_pool_price_v2(base_asset: Asset, quote_asset: Asset) -> Result<PoolPriceV2, DispatchErrorWithMessage> {
			Ok(
				LiquidityPools::pool_price(base_asset, quote_asset)?
					.map_sell_and_buy_prices(|price| price.sqrt_price)
			)
		}

		/// Simulates a swap and return the intermediate (if any) and final output.
		///
		/// If no swap rate can be calculated, returns None. This can happen if the pools are not
		/// provisioned, or if the input amount amount is too high or too low to give a meaningful
		/// output.
		///
		/// Note: This function must only be called through RPC, because RPC has its own storage buffer
		/// layer and would not affect on-chain storage.
		fn cf_pool_simulate_swap(
			input_asset: Asset,
			output_asset: Asset,
			input_amount: AssetAmount,
			broker_commission: BasisPoints,
			dca_parameters: Option<DcaParameters>,
			ccm_data: Option<CcmData>,
			exclude_fees: BTreeSet<FeeTypes>,
			additional_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
			is_internal: Option<bool>,
		) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage> {
			let is_internal = is_internal.unwrap_or_default();
			let mut exclude_fees = exclude_fees;
			if is_internal {
				exclude_fees.insert(FeeTypes::IngressDepositChannel);
				exclude_fees.insert(FeeTypes::Egress);
				exclude_fees.insert(FeeTypes::IngressVaultSwap);
			}

			if let Some(additional_orders) = additional_orders {
				for (index, additional_order) in additional_orders.into_iter().enumerate() {
					match additional_order {
						SimulateSwapAdditionalOrder::LimitOrder {
							base_asset,
							quote_asset,
							side,
							tick,
							sell_amount,
						} => {
							LiquidityPools::try_add_limit_order(
								&AccountId::new([0; 32]),
								base_asset,
								quote_asset,
								side,
								index as OrderId,
								tick,
								sell_amount.into(),
							)?;
						}
					}
				}
			}

			fn remove_fees(ingress_or_egress: IngressOrEgress, asset: Asset, amount: AssetAmount) -> (AssetAmount, AssetAmount) {
				use pallet_cf_ingress_egress::AmountAndFeesWithheld;

				match asset.into() {
					ForeignChainAndAsset::Ethereum(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees, fees_withheld)
					},
					ForeignChainAndAsset::Polkadot(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees, fees_withheld)
					},
					ForeignChainAndAsset::Bitcoin(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees.into(), fees_withheld.into())
					},
					ForeignChainAndAsset::Arbitrum(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees, fees_withheld)
					},
					ForeignChainAndAsset::Solana(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees.into(), fees_withheld.into())
					},
					ForeignChainAndAsset::Assethub(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, AssethubInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees, fees_withheld)
					},
				}
			}

			let include_fee = |fee_type: FeeTypes| !exclude_fees.contains(&fee_type);

			// Default to using the DepositChannel fee unless specified.
			let (amount_to_swap, ingress_fee) = if include_fee(FeeTypes::IngressDepositChannel) {
				remove_fees(IngressOrEgress::IngressDepositChannel, input_asset, input_amount)
			} else if include_fee(FeeTypes::IngressVaultSwap) {
				remove_fees(IngressOrEgress::IngressVaultSwap, input_asset, input_amount)
			}else {
				(input_amount, 0u128)
			};

			// Estimate swap result for a chunk, then extrapolate the result.
			// If no DCA parameter is given, swap the entire amount with 1 chunk.
			let number_of_chunks: u128 = dca_parameters.map(|dca|dca.number_of_chunks).unwrap_or(1u32).into();
			let amount_per_chunk = amount_to_swap / number_of_chunks;

			let mut fees_vec = vec![];

			if include_fee(FeeTypes::Network) {
				fees_vec.push(FeeType::NetworkFee(NetworkFeeTracker::new(
					pallet_cf_swapping::Pallet::<Runtime>::get_network_fee_for_swap(
						input_asset,
						output_asset,
						is_internal,
					),
				)));
			}

			if broker_commission > 0 {
				fees_vec.push(FeeType::BrokerFee(
					vec![Beneficiary {
						account: AccountId::new([0xbb; 32]),
						bps: broker_commission,
					}]
					.try_into()
					.expect("Beneficiary with a length of 1 must be within length bound.")
				));
			}

			// Simulate the swap
			let swap_output_per_chunk = Swapping::try_execute_without_violations(
				vec![
					Swap::new(
						Default::default(), // Swap id
						Default::default(), // Swap request id
						input_asset,
						output_asset,
						amount_per_chunk,
						None,
						fees_vec,
						Default::default(), // Execution block
					)
				],
			).map_err(|e| match e {
				BatchExecutionError::SwapLegFailed { .. } => DispatchError::Other("Swap leg failed."),
				BatchExecutionError::PriceViolation { .. } => DispatchError::Other("Price Violation: Some swaps failed due to Price Impact Limitations."),
				BatchExecutionError::DispatchError { error } => error,
			})?;

			let (
				network_fee,
				broker_fee,
				intermediary,
				output,
			) = {
				(
					swap_output_per_chunk[0].network_fee_taken.unwrap_or_default() * number_of_chunks,
					swap_output_per_chunk[0].broker_fee_taken.unwrap_or_default() * number_of_chunks,
					swap_output_per_chunk[0].stable_amount.map(|amount| amount * number_of_chunks)
						.filter(|_| ![input_asset, output_asset].contains(&STABLE_ASSET)),
					swap_output_per_chunk[0].final_output.unwrap_or_default() * number_of_chunks,
				)
			};

			let (output, egress_fee) = if include_fee(FeeTypes::Egress) {
				let egress = match ccm_data {
					Some(CcmData { gas_budget, message_length}) => {
						IngressOrEgress::EgressCcm {
							gas_budget,
							message_length: message_length as usize,
						}
					},
					None => IngressOrEgress::Egress,
				};
				remove_fees(egress, output_asset, output)
			} else {
				(output, 0u128)
			};


			Ok(SimulatedSwapInformation {
				intermediary,
				output,
				network_fee,
				ingress_fee,
				egress_fee,
				broker_fee,
			})
		}

		fn cf_pool_info(base_asset: Asset, quote_asset: Asset) -> Result<PoolInfo, DispatchErrorWithMessage> {
			LiquidityPools::pool_info(base_asset, quote_asset).map_err(Into::into)
		}

		fn cf_lp_events() -> Vec<pallet_cf_pools::Event<Runtime>> {
			System::read_events_no_consensus().filter_map(|event_record| {
				if let RuntimeEvent::LiquidityPools(pools_event) = event_record.event {
					Some(pools_event)
				} else {
					None
				}
			}).collect()

		}

		fn cf_pool_depth(base_asset: Asset, quote_asset: Asset, tick_range: Range<cf_amm::math::Tick>) -> Result<AskBidMap<UnidirectionalPoolDepth>, DispatchErrorWithMessage> {
			LiquidityPools::pool_depth(base_asset, quote_asset, tick_range).map_err(Into::into)
		}

		fn cf_pool_liquidity(base_asset: Asset, quote_asset: Asset) -> Result<PoolLiquidity, DispatchErrorWithMessage> {
			LiquidityPools::pool_liquidity(base_asset, quote_asset).map_err(Into::into)
		}

		fn cf_required_asset_ratio_for_range_order(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::math::Tick>,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage> {
			LiquidityPools::required_asset_ratio_for_range_order(base_asset, quote_asset, tick_range).map_err(Into::into)
		}

		fn cf_pool_orderbook(
			base_asset: Asset,
			quote_asset: Asset,
			orders: u32,
		) -> Result<PoolOrderbook, DispatchErrorWithMessage> {
			LiquidityPools::pool_orderbook(base_asset, quote_asset, orders).map_err(Into::into)
		}

		fn cf_pool_orders(
			base_asset: Asset,
			quote_asset: Asset,
			lp: Option<AccountId>,
			filled_orders: bool,
		) -> Result<PoolOrders<Runtime>, DispatchErrorWithMessage> {
			LiquidityPools::pool_orders(base_asset, quote_asset, lp, filled_orders).map_err(Into::into)
		}

		fn cf_pool_range_order_liquidity_value(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<Tick>,
			liquidity: Liquidity,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage> {
			LiquidityPools::pool_range_order_liquidity_value(base_asset, quote_asset, tick_range, liquidity).map_err(Into::into)
		}

		fn cf_network_environment() -> NetworkEnvironment {
			Environment::network_environment()
		}

		fn cf_max_swap_amount(asset: Asset) -> Option<AssetAmount> {
			Swapping::maximum_swap_amount(asset)
		}

		fn cf_min_deposit_amount(asset: Asset) -> AssetAmount {
			chainflip::MinimumDepositProvider::get(asset)
		}

		fn cf_egress_dust_limit(generic_asset: Asset) -> AssetAmount {
			use pallet_cf_ingress_egress::EgressDustLimit;

			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => EgressDustLimit::<Runtime, EthereumInstance>::get(asset),
				ForeignChainAndAsset::Polkadot(asset) => EgressDustLimit::<Runtime, PolkadotInstance>::get(asset),
				ForeignChainAndAsset::Bitcoin(asset) => EgressDustLimit::<Runtime, BitcoinInstance>::get(asset),
				ForeignChainAndAsset::Arbitrum(asset) => EgressDustLimit::<Runtime, ArbitrumInstance>::get(asset),
				ForeignChainAndAsset::Solana(asset) => EgressDustLimit::<Runtime, SolanaInstance>::get(asset),
				ForeignChainAndAsset::Assethub(asset) => EgressDustLimit::<Runtime, AssethubInstance>::get(asset),
			}
		}

		fn cf_ingress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)
					))
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel).into()),
				ForeignChainAndAsset::Arbitrum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)
					))
				},
				ForeignChainAndAsset::Solana(asset) => Some(SolanaChainTrackingProvider::estimate_fee(asset, IngressOrEgress::IngressDepositChannel).into()),
				ForeignChainAndAsset::Assethub(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Assethub>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, AssethubInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)
					))
				},
			}
		}

		fn cf_egress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_fee(asset, IngressOrEgress::Egress)
					))
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_fee(asset, IngressOrEgress::Egress)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_fee(asset, IngressOrEgress::Egress).into()),
				ForeignChainAndAsset::Arbitrum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_fee(asset, IngressOrEgress::Egress)
					))
				},
				ForeignChainAndAsset::Solana(asset) => Some(SolanaChainTrackingProvider::estimate_fee(asset, IngressOrEgress::Egress).into()),
				ForeignChainAndAsset::Assethub(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Assethub>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, AssethubInstance>::estimate_fee(asset, IngressOrEgress::Egress)
					))
				},
			}
		}

		fn cf_witness_safety_margin(chain: ForeignChain) -> Option<u64> {
			match chain {
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::witness_safety_margin(),
				ForeignChain::Ethereum => pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::witness_safety_margin(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::witness_safety_margin().map(Into::into),
				ForeignChain::Arbitrum => pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::witness_safety_margin(),
				ForeignChain::Solana => pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::witness_safety_margin(),
				ForeignChain::Assethub => pallet_cf_ingress_egress::Pallet::<Runtime, AssethubInstance>::witness_safety_margin().map(Into::into),
			}
		}

		fn cf_liquidity_provider_info(
			account_id: AccountId,
		) -> LiquidityProviderInfo {
			let refund_addresses = ForeignChain::iter().map(|chain| {
				(chain, pallet_cf_lp::LiquidityRefundAddress::<Runtime>::get(&account_id, chain))
			}).collect();

			LiquidityPools::sweep(&account_id).unwrap();

			LiquidityProviderInfo {
				refund_addresses,
				balances: Asset::all().map(|asset|
					(asset, pallet_cf_asset_balances::FreeBalances::<Runtime>::get(&account_id, asset))
				).collect(),
				earned_fees: AssetMap::from_iter(HistoricalEarnedFees::<Runtime>::iter_prefix(&account_id)),
				boost_balances: AssetMap::from_fn(|asset| {
					let pool_details = Self::cf_boost_pool_details(asset);

					pool_details.into_iter().filter_map(|(fee_tier, details)| {
						let available_balance = details.available_amounts.into_iter().find_map(|(id, amount)| {
							if id == account_id {
								Some(amount)
							} else {
								None
							}
						}).unwrap_or(0);

						let owed_amount = details.pending_boosts.into_iter().flat_map(|(_, pending_deposits)| {
							pending_deposits.into_iter().filter_map(|(id, amount)| {
								if id == account_id {
									Some(amount.total)
								} else {
									None
								}
							})
						}).sum();

						let total_balance = available_balance + owed_amount;

						if total_balance == 0 {
							return None
						}

						Some(LiquidityProviderBoostPoolInfo {
							fee_tier,
							total_balance,
							available_balance,
							in_use_balance: owed_amount,
							is_withdrawing: details.pending_withdrawals.keys().any(|id| *id == account_id),
						})
					}).collect()
				}),
			}
		}

		fn cf_broker_info(
			account_id: AccountId,
		) -> BrokerInfo {
			let account_info = pallet_cf_flip::Account::<Runtime>::get(&account_id);
			BrokerInfo {
				earned_fees: Asset::all().map(|asset|
					(asset, AssetBalances::get_balance(&account_id, asset))
				).collect(),
				btc_vault_deposit_address: BrokerPrivateBtcChannels::<Runtime>::get(&account_id)
					.map(|channel| derive_btc_vault_deposit_addresses(channel).current_address()),
				affiliates: pallet_cf_swapping::AffiliateAccountDetails::<Runtime>::iter_prefix(&account_id).collect(),
				bond: account_info.bond()
			}
		}

		fn cf_account_role(account_id: AccountId) -> Option<AccountRole> {
			pallet_cf_account_roles::AccountRoles::<Runtime>::get(account_id)
		}

		fn cf_redemption_tax() -> AssetAmount {
			pallet_cf_funding::RedemptionTax::<Runtime>::get()
		}

		fn cf_swap_retry_delay_blocks() -> u32 {
			pallet_cf_swapping::SwapRetryDelay::<Runtime>::get()
		}

		fn cf_swap_limits() -> SwapLimits {
			pallet_cf_swapping::Pallet::<Runtime>::get_swap_limits()
		}

		fn cf_minimum_chunk_size(asset: Asset) -> AssetAmount {
			Swapping::minimum_chunk_size(asset)
		}

		fn cf_scheduled_swaps(base_asset: Asset, quote_asset: Asset) -> Vec<(SwapLegInfo, BlockNumber)> {
			assert_eq!(quote_asset, STABLE_ASSET, "Only USDC is supported as quote asset");
			Swapping::get_scheduled_swap_legs(base_asset)
		}

		fn cf_failed_call_ethereum(broadcast_id: BroadcastId) -> Option<<cf_chains::Ethereum as cf_chains::Chain>::Transaction> {
			if EthereumIngressEgress::get_failed_call(broadcast_id).is_some() {
				EthereumBroadcaster::threshold_signature_data(broadcast_id).map(|api_call|{
					chainflip::EthTransactionBuilder::build_transaction(&api_call)
				})
			} else {
				None
			}
		}

		fn cf_failed_call_arbitrum(broadcast_id: BroadcastId) -> Option<<cf_chains::Arbitrum as cf_chains::Chain>::Transaction> {
			if ArbitrumIngressEgress::get_failed_call(broadcast_id).is_some() {
				ArbitrumBroadcaster::threshold_signature_data(broadcast_id).map(|api_call|{
					chainflip::ArbTransactionBuilder::build_transaction(&api_call)
				})
			} else {
				None
			}
		}

		fn cf_witness_count(hash: pallet_cf_witnesser::CallHash, epoch_index: Option<EpochIndex>) -> Option<FailingWitnessValidators> {
			let mut result: FailingWitnessValidators = FailingWitnessValidators {
				failing_count: 0,
				validators: vec![],
			};
			let voting_validators = Witnesser::count_votes(epoch_index.unwrap_or(<Runtime as Chainflip>::EpochInfo::current_epoch()), hash);
			let vanity_names: BTreeMap<AccountId, BoundedVec<u8, _>> = pallet_cf_account_roles::VanityNames::<Runtime>::get();
			voting_validators?.iter().for_each(|(val, voted)| {
				let vanity = vanity_names.get(val).cloned().unwrap_or_default();
				if !voted {
					result.failing_count += 1;
				}
				result.validators.push((val.clone(), String::from_utf8_lossy(&vanity).into(), *voted));
			});

			Some(result)
		}

		fn cf_channel_opening_fee(chain: ForeignChain) -> FlipBalance {
			match chain {
				ForeignChain::Ethereum => pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::channel_opening_fee(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::channel_opening_fee(),
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::channel_opening_fee(),
				ForeignChain::Arbitrum => pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::channel_opening_fee(),
				ForeignChain::Solana => pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::channel_opening_fee(),
				ForeignChain::Assethub => pallet_cf_ingress_egress::Pallet::<Runtime, AssethubInstance>::channel_opening_fee(),
			}
		}

		fn cf_boost_pools_depth() -> Vec<BoostPoolDepth> {

			pallet_cf_lending_pools::boost_pools_iter::<Runtime>().map(|(asset, tier, core_pool)| {

				BoostPoolDepth {
					asset,
					tier,
					available_amount: core_pool.get_available_amount()
				}

			}).collect()

		}

		fn cf_boost_pool_details(asset: Asset) -> BTreeMap<u16, BoostPoolDetails<AccountId>> {
			pallet_cf_lending_pools::get_boost_pool_details::<Runtime>(asset)
		}

		fn cf_safe_mode_statuses() -> RuntimeSafeMode {
			pallet_cf_environment::RuntimeSafeMode::<Runtime>::get()
		}

		fn cf_pools() -> Vec<PoolPairsMap<Asset>> {
			LiquidityPools::pools()
		}

		fn cf_validate_dca_params(number_of_chunks: u32, chunk_interval: u32) -> Result<(), DispatchErrorWithMessage> {
			pallet_cf_swapping::Pallet::<Runtime>::validate_dca_params(&DcaParameters{number_of_chunks, chunk_interval}).map_err(Into::into)
		}

		fn cf_validate_refund_params(
			input_asset: Asset,
			output_asset: Asset,
			retry_duration: BlockNumber,
			max_oracle_price_slippage: Option<BasisPoints>,
		) -> Result<(), DispatchErrorWithMessage> {
			pallet_cf_swapping::Pallet::<Runtime>::validate_refund_params(
				input_asset,
				output_asset,
				retry_duration,
				max_oracle_price_slippage,
			)
			.map_err(Into::into)
		}

		fn cf_request_swap_parameter_encoding(
			broker: AccountId,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			extra_parameters: VaultSwapExtraParametersEncoded,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
			boost_fee: BasisPoints,
			affiliate_fees: Affiliates<AccountId>,
			dca_parameters: Option<DcaParameters>,
		) -> Result<VaultSwapDetails<String>, DispatchErrorWithMessage> {
			let source_chain = ForeignChain::from(source_asset);
			let destination_chain = ForeignChain::from(destination_asset);


			// Validate refund params
			let (retry_duration, max_oracle_price_slippage) = match &extra_parameters {
				VaultSwapExtraParametersEncoded::Bitcoin { retry_duration, max_oracle_price_slippage, .. } => {
					let max_oracle_price_slippage = match max_oracle_price_slippage {
						Some(slippage) if *slippage == u8::MAX => None,
						Some(slippage) => Some((*slippage).into()),
						None => None,
					};
					(*retry_duration, max_oracle_price_slippage)
				},
				VaultSwapExtraParametersEncoded::Ethereum(EvmVaultSwapExtraParameters { refund_parameters, .. }) => {
					refund_parameters.clone().try_map_refund_address_to_foreign_chain_address::<ChainAddressConverter>()?.into_checked(None, source_asset)?;
					(refund_parameters.retry_duration, refund_parameters.max_oracle_price_slippage)
				},
				VaultSwapExtraParametersEncoded::Arbitrum(EvmVaultSwapExtraParameters { refund_parameters, .. }) => {
					refund_parameters.clone().try_map_refund_address_to_foreign_chain_address::<ChainAddressConverter>()?.into_checked(None, source_asset)?;
					(refund_parameters.retry_duration, refund_parameters.max_oracle_price_slippage)
				},
				VaultSwapExtraParametersEncoded::Solana { refund_parameters, .. } => {
					refund_parameters.clone().try_map_refund_address_to_foreign_chain_address::<ChainAddressConverter>()?.into_checked(None, source_asset)?;
					(refund_parameters.retry_duration, refund_parameters.max_oracle_price_slippage)
				},
			};

			let checked_ccm = crate::chainflip::vault_swaps::validate_parameters(
				&broker,
				source_asset,
				&destination_address,
				destination_asset,
				&dca_parameters,
				boost_fee,
				broker_commission,
				&affiliate_fees,
				retry_duration,
				&channel_metadata,
				max_oracle_price_slippage,
			)?;

			// Conversion implicitly verifies address validity.
			frame_support::ensure!(
				ChainAddressConverter::try_from_encoded_address(destination_address.clone())
					.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDestinationAddress)?
					.chain() == destination_chain
				,
				"Destination address and asset are on different chains."
			);

			// Convert boost fee.
			let boost_fee: u8 = boost_fee
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh)?;

			// Validate broker fee
			if broker_commission < pallet_cf_swapping::Pallet::<Runtime>::get_minimum_vault_swap_fee_for_broker(&broker) {
				return Err(DispatchErrorWithMessage::from("Broker commission is too low"));
			}
			let _beneficiaries = pallet_cf_swapping::Pallet::<Runtime>::assemble_and_validate_broker_fees(
				broker.clone(),
				broker_commission,
				affiliate_fees.clone(),
			)?;

			// Encode swap
			match (source_chain, extra_parameters) {
				(
					ForeignChain::Bitcoin,
					VaultSwapExtraParameters::Bitcoin {
						min_output_amount,
						retry_duration,
						max_oracle_price_slippage,
					}
				) => {
					crate::chainflip::vault_swaps::bitcoin_vault_swap(
						broker,
						destination_asset,
						destination_address,
						broker_commission,
						min_output_amount,
						retry_duration,
						boost_fee,
						affiliate_fees,
						dca_parameters,
						max_oracle_price_slippage,
					)
				},
				(
					ForeignChain::Ethereum,
					VaultSwapExtraParametersEncoded::Ethereum(extra_params)
				)|
				(
					ForeignChain::Arbitrum,
					VaultSwapExtraParametersEncoded::Arbitrum(extra_params)
				) => {
					crate::chainflip::vault_swaps::evm_vault_swap(
						broker,
						source_asset,
						extra_params.input_amount,
						destination_asset,
						destination_address,
						broker_commission,
						extra_params.refund_parameters,
						boost_fee,
						affiliate_fees,
						dca_parameters,
						checked_ccm,
					)
				},
				(
					ForeignChain::Solana,
					VaultSwapExtraParameters::Solana {
						from,
						seed,
						input_amount,
						refund_parameters,
						from_token_account,
					}
				) => crate::chainflip::vault_swaps::solana_vault_swap(
					broker,
					input_amount,
					source_asset,
					destination_asset,
					destination_address,
					broker_commission,
					refund_parameters,
					checked_ccm,
					boost_fee,
					affiliate_fees,
					dca_parameters,
					from,
					seed,
					from_token_account,
				),
				_ => Err(DispatchErrorWithMessage::from(
					"Incompatible or unsupported source_asset and extra_parameters"
				)),
			}
		}

		fn cf_decode_vault_swap_parameter(
			broker: AccountId,
			vault_swap: VaultSwapDetails<String>,
		) -> Result<VaultSwapInputEncoded, DispatchErrorWithMessage> {
			match vault_swap {
				VaultSwapDetails::Bitcoin {
					nulldata_payload,
					deposit_address: _,
				} => {
					crate::chainflip::vault_swaps::decode_bitcoin_vault_swap(
						broker,
						nulldata_payload,
					)
				},
				VaultSwapDetails::Solana {
					instruction,
				} => {
					crate::chainflip::vault_swaps::decode_solana_vault_swap(
						instruction.into(),
					)
				},
				_ => Err(DispatchErrorWithMessage::from(
					"Decoding Vault Swap only supports Bitcoin and Solana"
				)),
			}
		}

		fn cf_encode_cf_parameters(
			broker: AccountId,
			source_asset: Asset,
			destination_address: EncodedAddress,
			destination_asset: Asset,
			refund_parameters: ChannelRefundParametersUncheckedEncoded,
			dca_parameters: Option<DcaParameters>,
			boost_fee: BasisPoints,
			broker_commission: BasisPoints,
			affiliate_fees: Affiliates<AccountId>,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
		) -> Result<Vec<u8>, DispatchErrorWithMessage> {
			// Validate the parameters
			let checked_ccm = crate::chainflip::vault_swaps::validate_parameters(
				&broker,
				source_asset,
				&destination_address,
				destination_asset,
				&dca_parameters,
				boost_fee,
				broker_commission,
				&affiliate_fees,
				refund_parameters.retry_duration,
				&channel_metadata,
				refund_parameters.max_oracle_price_slippage,
			)?;

			let boost_fee: u8 = boost_fee
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh)?;

			let affiliate_and_fees = crate::chainflip::vault_swaps::to_affiliate_and_fees(&broker, affiliate_fees)?
				.try_into()
				.map_err(|_| "Too many affiliates.")?;

			macro_rules! build_and_encode_cf_parameters_for_chain {
				($chain:ty) => {
					build_and_encode_cf_parameters::<<$chain as cf_chains::Chain>::ChainAccount>(
						refund_parameters.try_map_address(|addr| {
							Ok::<_, DispatchErrorWithMessage>(
								ChainAddressConverter::try_from_encoded_address(addr)
									.and_then(|addr| addr.try_into().map_err(|_| ()))
									.map_err(|_| "Invalid refund address")?,
							)
						})?,
						dca_parameters,
						boost_fee,
						broker,
						broker_commission,
						affiliate_and_fees,
						checked_ccm.as_ref(),
					)
				}
			}

			Ok(match ForeignChain::from(source_asset) {
				ForeignChain::Ethereum => build_and_encode_cf_parameters_for_chain!(Ethereum),
				ForeignChain::Arbitrum => build_and_encode_cf_parameters_for_chain!(Arbitrum),
				ForeignChain::Solana => build_and_encode_cf_parameters_for_chain!(Solana),
				_ => Err(DispatchErrorWithMessage::from("Unsupported source chain for encoding cf_parameters"))?,
			})
		}

		fn cf_get_preallocated_deposit_channels(account_id: <Runtime as frame_system::Config>::AccountId, chain: ForeignChain) -> Vec<ChannelId> {

			fn preallocated_deposit_channels_for_chain<T: pallet_cf_ingress_egress::Config<I>, I: 'static>(
				account_id: &<T as frame_system::Config>::AccountId,
			) -> Vec<ChannelId>
			{
				pallet_cf_ingress_egress::PreallocatedChannels::<T, I>::get(account_id).iter()
					.map(|channel| channel.channel_id)
					.collect()
			}

			match chain {
				ForeignChain::Bitcoin => preallocated_deposit_channels_for_chain::<Runtime, BitcoinInstance>(&account_id),
				ForeignChain::Ethereum => preallocated_deposit_channels_for_chain::<Runtime, EthereumInstance>(&account_id),
				ForeignChain::Polkadot => preallocated_deposit_channels_for_chain::<Runtime, PolkadotInstance>(&account_id),
				ForeignChain::Arbitrum => preallocated_deposit_channels_for_chain::<Runtime, ArbitrumInstance>(&account_id),
				ForeignChain::Solana => preallocated_deposit_channels_for_chain::<Runtime, SolanaInstance>(&account_id),
				ForeignChain::Assethub => preallocated_deposit_channels_for_chain::<Runtime, AssethubInstance>(&account_id),
			}
		}

		fn cf_get_open_deposit_channels(account_id: Option<<Runtime as frame_system::Config>::AccountId>) -> ChainAccounts {
			fn open_deposit_channels_for_account<T: pallet_cf_ingress_egress::Config<I>, I: 'static>(
				account_id: Option<&<T as frame_system::Config>::AccountId>
			) -> Vec<EncodedAddress>
			{
				let network_environment = Environment::network_environment();
				pallet_cf_ingress_egress::DepositChannelLookup::<T, I>::iter_values()
					.filter(|channel_details| account_id.is_none() || Some(&channel_details.owner) == account_id)
					.map(|channel_details|
						channel_details.deposit_channel.address
							.into_foreign_chain_address()
							.to_encoded_address(network_environment)
					)
					.collect::<Vec<_>>()
			}

			ChainAccounts {
				chain_accounts: [
					open_deposit_channels_for_account::<Runtime, BitcoinInstance>(account_id.as_ref()),
					open_deposit_channels_for_account::<Runtime, EthereumInstance>(account_id.as_ref()),
					open_deposit_channels_for_account::<Runtime, ArbitrumInstance>(account_id.as_ref()),
				].into_iter().flatten().collect()
			}
		}

		fn cf_all_open_deposit_channels() -> Vec<OpenedDepositChannels> {
			use sp_std::collections::btree_set::BTreeSet;

			#[allow(clippy::type_complexity)]
			fn open_deposit_channels_for_chain_instance<T: pallet_cf_ingress_egress::Config<I>, I: 'static>()
				-> BTreeMap<(<T as frame_system::Config>::AccountId, ChannelActionType), Vec<EncodedAddress>>
			{
				let network_environment = Environment::network_environment();
				pallet_cf_ingress_egress::DepositChannelLookup::<T, I>::iter_values()
					.fold(BTreeMap::new(), |mut acc, channel_details| {
						acc.entry((channel_details.owner.clone(), channel_details.action.into()))
							.or_default()
							.push(
								channel_details.deposit_channel.address
								.into_foreign_chain_address()
								.to_encoded_address(network_environment)
							);
						acc
					})
			}

			let btc_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, BitcoinInstance>();
			let eth_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, EthereumInstance>();
			let arb_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, ArbitrumInstance>();
			let accounts = btc_chain_accounts.keys()
				.chain(eth_chain_accounts.keys())
				.chain(arb_chain_accounts.keys())
				.cloned().collect::<BTreeSet<_>>();

			accounts.into_iter().map(|key| {
				let (account_id, channel_action_type) = key.clone();
				(account_id, channel_action_type, ChainAccounts {
					chain_accounts: [
						btc_chain_accounts.get(&key).cloned().unwrap_or_default(),
						eth_chain_accounts.get(&key).cloned().unwrap_or_default(),
						arb_chain_accounts.get(&key).cloned().unwrap_or_default(),
					].into_iter().flatten().collect()
				})
			}).collect()
		}

		fn cf_transaction_screening_events() -> crate::runtime_apis::TransactionScreeningEvents {
			use crate::runtime_apis::BrokerRejectionEventFor;
			fn extract_screening_events<
				T: pallet_cf_ingress_egress::Config<I, AccountId = <Runtime as frame_system::Config>::AccountId>,
				I: 'static
			>(
				event: pallet_cf_ingress_egress::Event::<T, I>,
			) -> Vec<BrokerRejectionEventFor<T::TargetChain>> {
				use cf_chains::DepositDetailsToTransactionInId;
				match event {
					pallet_cf_ingress_egress::Event::TransactionRejectionRequestExpired { account_id, tx_id } =>
						vec![TransactionScreeningEvent::TransactionRejectionRequestExpired { account_id, tx_id }],
					pallet_cf_ingress_egress::Event::TransactionRejectionRequestReceived { account_id, tx_id, expires_at: _ } =>
						vec![TransactionScreeningEvent::TransactionRejectionRequestReceived { account_id, tx_id }],
					pallet_cf_ingress_egress::Event::TransactionRejectedByBroker { broadcast_id, tx_id } => tx_id
						.deposit_ids()
						.into_iter()
						.flat_map(IntoIterator::into_iter)
						.map(|tx_id|
							TransactionScreeningEvent::TransactionRejectedByBroker { refund_broadcast_id: broadcast_id, tx_id }
						)
						.collect(),
					_ => Default::default(),
				}
			}

			let mut btc_events: Vec<BrokerRejectionEventFor<cf_chains::Bitcoin>> = Default::default();
			let mut eth_events: Vec<BrokerRejectionEventFor<cf_chains::Ethereum>> = Default::default();
			let mut arb_events: Vec<BrokerRejectionEventFor<cf_chains::Arbitrum>> = Default::default();
			for event_record in System::read_events_no_consensus() {
				match event_record.event {
					RuntimeEvent::BitcoinIngressEgress(event) => btc_events.extend(extract_screening_events::<Runtime, BitcoinInstance>(event)),
					RuntimeEvent::EthereumIngressEgress(event) => eth_events.extend(extract_screening_events::<Runtime, EthereumInstance>(event)),
					RuntimeEvent::ArbitrumIngressEgress(event) => arb_events.extend(extract_screening_events::<Runtime, ArbitrumInstance>(event)),
					_ => {},
				}
			}

			TransactionScreeningEvents {
				btc_events,
				eth_events,
				arb_events,
			}
		}

		fn cf_affiliate_details(
			broker: AccountId,
			affiliate: Option<AccountId>,
		) -> Vec<(AccountId, AffiliateDetails)>{
			if let Some(affiliate) = affiliate {
				pallet_cf_swapping::AffiliateAccountDetails::<Runtime>::get(&broker, &affiliate)
					.map(|details| (affiliate, details))
					.into_iter()
					.collect()
			} else {
				pallet_cf_swapping::AffiliateAccountDetails::<Runtime>::iter_prefix(&broker).collect()
			}
		}

		fn cf_vault_addresses() -> VaultAddresses {
			VaultAddresses {
				ethereum: EncodedAddress::Eth(Environment::eth_vault_address().into()),
				arbitrum: EncodedAddress::Arb(Environment::arb_vault_address().into()),
				bitcoin: BrokerPrivateBtcChannels::<Runtime>::iter()
					.flat_map(|(account_id, channel_id)| {
						let BitcoinPrivateBrokerDepositAddresses { previous, current } = derive_btc_vault_deposit_addresses(channel_id)
							.with_encoded_addresses();
						previous.into_iter().chain(core::iter::once(current))
							.map(move |address| (account_id.clone(), address))
					})
					.collect(),
			}
		}

		fn cf_get_trading_strategies(lp_id: Option<AccountId>,) -> Vec<TradingStrategyInfo<AssetAmount>> {

			type Strategies = pallet_cf_trading_strategy::Strategies::<Runtime>;
			type Strategy = pallet_cf_trading_strategy::TradingStrategy;

			fn to_strategy_info(lp_id: AccountId, strategy_id: AccountId, strategy: Strategy) -> TradingStrategyInfo<AssetAmount> {

				LiquidityPools::sweep(&strategy_id).unwrap();

				let free_balances = AssetBalances::free_balances(&strategy_id);
				let open_order_balances = LiquidityPools::open_order_balances(&strategy_id);

				let total_balances = free_balances.saturating_add(open_order_balances);

				let supported_assets = strategy.supported_assets();
				let supported_asset_balances = total_balances.iter()
					.filter(|(asset, _amount)| supported_assets.contains(asset))
					.map(|(asset, amount)| (asset, *amount));

				TradingStrategyInfo {
					lp_id,
					strategy_id,
					strategy,
					balance: supported_asset_balances.collect(),
				}

			}

			if let Some(lp_id) = &lp_id {
				Strategies::iter_prefix(lp_id).map(|(strategy_id, strategy)| to_strategy_info(lp_id.clone(), strategy_id, strategy)).collect()
			} else {
				Strategies::iter().map(|(lp_id, strategy_id, strategy)| to_strategy_info(lp_id, strategy_id, strategy)).collect()
			}

		}

		fn cf_trading_strategy_limits() -> TradingStrategyLimits{
			TradingStrategyLimits{
				minimum_deployment_amount: AssetMap::from_iter(pallet_cf_trading_strategy::MinimumDeploymentAmountForStrategy::<Runtime>::get().into_iter()
					.map(|(asset, balance)| (asset, Some(balance)))),
				minimum_added_funds_amount: AssetMap::from_iter(pallet_cf_trading_strategy::MinimumAddedFundsToStrategy::<Runtime>::get().into_iter()
					.map(|(asset, balance)| (asset, Some(balance)))),
			}
		}

		fn cf_network_fees() -> NetworkFees{
			let regular_network_fee = pallet_cf_swapping::NetworkFee::<Runtime>::get();
			let internal_swap_network_fee = pallet_cf_swapping::InternalSwapNetworkFee::<Runtime>::get();
			NetworkFees {
				regular_network_fee: NetworkFeeDetails{
					rates: AssetMap::from_fn(|asset|{
						pallet_cf_swapping::NetworkFeeForAsset::<Runtime>::get(asset).unwrap_or(regular_network_fee.rate)
					}),
					standard_rate_and_minimum: regular_network_fee,
				},
				internal_swap_network_fee: NetworkFeeDetails{
					rates: AssetMap::from_fn(|asset|{
						pallet_cf_swapping::InternalSwapNetworkFeeForAsset::<Runtime>::get(asset).unwrap_or(internal_swap_network_fee.rate)
					}),
					standard_rate_and_minimum: internal_swap_network_fee,
				},
			}
		}

		fn cf_oracle_prices(base_and_quote_asset: Option<(PriceAsset, PriceAsset)>,) -> Vec<OraclePrice> {
			if let Some(state) = pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, ()>::get() {
				get_latest_oracle_prices(&state.0, base_and_quote_asset)
			} else {
				vec![]
			}
		}

		fn cf_evm_calldata(
			caller: EthereumAddress,
			call: EthereumSCApi<FlipBalance>,
		) -> Result<EvmCallDetails, DispatchErrorWithMessage> {
			use chainflip::ethereum_sc_calls::DelegationApi;
			let caller_id = EthereumAccount(caller).into_account_id();
			let required_deposit = match call {
				EthereumSCApi::Delegation { call: DelegationApi::Delegate { increase: DelegationAmount::Some(ref increase), .. } } => {
					pallet_cf_validator::DelegationChoice::<Runtime>::get(&caller_id).map(|(_, bid)| bid).unwrap_or_default()
						.saturating_add(*increase)
						.saturating_sub(pallet_cf_flip::Pallet::<Runtime>::balance(&caller_id))
				},
				_ => 0,
			};
			Ok(EvmCallDetails {
				calldata: if required_deposit > 0 {
					DepositToSCGatewayAndCall::new(required_deposit, call.encode()).abi_encoded_payload()
				} else {
					SCCall::new(call.encode()).abi_encoded_payload()
				},
				value: U256::zero(),
				to: Environment::eth_sc_utils_address(),
				source_token_address: if required_deposit > 0 {
					Some(
						Environment::supported_eth_assets(cf_primitives::chains::assets::eth::Asset::Flip)
							.ok_or(DispatchErrorWithMessage::from(
								"flip token address not found on the state chain: {e}",
							))?
					)
				} else {
					None
				},
			})
		}
		fn cf_active_delegations(operator: Option<AccountId>) -> Vec<DelegationSnapshot<AccountId, FlipBalance>> {
			let current_epoch = Validator::current_epoch();
			if let Some(account_id) = operator {
				pallet_cf_validator::DelegationSnapshots::<Runtime>::get(current_epoch, &account_id).into_iter().collect()
			} else {
				pallet_cf_validator::DelegationSnapshots::<Runtime>::iter_prefix_values(current_epoch)
					.collect()
			}
		}
	}
}
