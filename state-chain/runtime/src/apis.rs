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

// Pallet imports
use crate::{
	AccountRoles, ArbitrumBroadcaster, ArbitrumIngressEgress, AssetBalances, Aura, Emissions,
	Environment, EthereumBroadcaster, EthereumIngressEgress, EthereumVault, EvmThresholdSigner,
	Flip, Governance, Grandpa, InherentDataExt, LiquidityPools, PolkadotThresholdSigner,
	SolanaBroadcaster, SolanaElections, SolanaThresholdSigner, Swapping, System, Validator,
	Witnesser,
};

use crate::{
	chainflip::{
		self,
		address_derivation::btc::{
			derive_btc_vault_deposit_addresses, BitcoinPrivateBrokerDepositAddresses,
		},
		calculate_account_apy,
		solana_elections::SolanaChainTrackingProvider,
		Offence,
	},
	monitoring_apis,
	monitoring_apis::{
		ActivateKeysBroadcastIds, AuthoritiesInfo, BtcUtxos, EpochState, ExternalChainsBlockHeight,
		FeeImbalance, FlipSupply, LastRuntimeUpgradeInfo, OpenDepositChannels, PendingBroadcasts,
		PendingTssCeremonies, RedemptionsInfo, SolanaNonces,
	},
	opaque, runtime_apis,
	runtime_apis::{
		runtime_decl_for_custom_runtime_api::CustomRuntimeApi, AuctionState, BoostPoolDepth,
		BoostPoolDetails, BrokerInfo, CcmData, ChannelActionType, DispatchErrorWithMessage,
		FailingWitnessValidators, FeeTypes, LiquidityProviderBoostPoolInfo, LiquidityProviderInfo,
		NetworkFeeDetails, NetworkFees, RuntimeApiPenalty, SimulateSwapAdditionalOrder,
		SimulatedSwapInformation, TradingStrategyInfo, TradingStrategyLimits,
		TransactionScreeningEvent, TransactionScreeningEvents, ValidatorInfo, VaultAddresses,
		VaultSwapDetails,
	},
	Executive, Historical, RuntimeGenesisConfig, RuntimeSafeMode, TransactionPayment,
};
use cf_amm::{
	common::PoolPairsMap,
	math::{Amount, Tick},
	range_orders::Liquidity,
};
pub use cf_chains::instances::{
	ArbitrumInstance, AssethubInstance, BitcoinInstance, EthereumInstance, EvmInstance,
	PolkadotCryptoInstance, PolkadotInstance, SolanaInstance,
};
use cf_chains::{
	address::{AddressConverter, EncodedAddress, IntoForeignChainAddress},
	assets::any::{AssetMap, ForeignChainAndAsset},
	btc::{api::BitcoinApi, ScriptPubkey},
	ccm_checker::{check_ccm_for_blacklisted_accounts, DecodedCcmAdditionalData},
	cf_parameters::build_cf_parameters,
	dot::PolkadotAccountId,
	eth::Ethereum,
	evm::Address as EvmAddress,
	sol::{api::SolanaEnvironment, SolPubkey},
	Arbitrum, Assethub, CcmChannelMetadataUnchecked, ChannelRefundParametersEncoded, ForeignChain,
	Solana, TransactionBuilder, VaultSwapExtraParameters, VaultSwapExtraParametersEncoded,
	VaultSwapInputEncoded,
};
use cf_primitives::{
	AccountRole, Affiliates, Asset, AssetAmount, BasisPoints, Beneficiary, BlockNumber,
	BroadcastId, DcaParameters, EpochIndex, FlipBalance, NetworkEnvironment, SemVer, STABLE_ASSET,
};
use cf_traits::{
	AdjustedFeeEstimationApi, AssetConverter, BalanceApi, BoostApi, Chainflip, EpochInfo, EpochKey,
	GetBlockHeight, KeyProvider, MinimumDeposit, OrderId, PoolApi, QualifyNode, SwapLimits,
	SwapParameterValidation,
};
use chainflip::{boost_api::IngressEgressBoostApi, ChainAddressConverter, SolEnvironment};
use codec::{alloc::string::ToString, Decode, Encode};
use core::ops::Range;
use frame_support::pallet_prelude::*;
pub use frame_system::Call as SystemCall;
use monitoring_apis::MonitoringDataV2;
use pallet_cf_funding::{MinimumFunding, RedemptionAmount};
use pallet_cf_governance::GovCallHash;
use pallet_cf_ingress_egress::{IngressOrEgress, OwedAmount, TargetChainAsset};
use pallet_cf_pools::{
	AskBidMap, HistoricalEarnedFees, PoolInfo, PoolLiquidity, PoolOrderbook, PoolOrders,
	PoolPriceV1, PoolPriceV2, UnidirectionalPoolDepth,
};
use pallet_cf_reputation::HeartbeatQualification;
use pallet_cf_swapping::{
	AffiliateDetails, BatchExecutionError, BrokerPrivateBtcChannels, FeeType, NetworkFeeTracker,
	Swap, SwapLegInfo,
};
use pallet_cf_validator::SetSizeMaximisingAuctionResolver;
use runtime_apis::ChainAccounts;
use scale_info::prelude::string::String;
use sol_prim::Address as SolAddress;
use sp_runtime::{traits::UniqueSaturatedInto, Saturating};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	prelude::*,
};

use crate::{AccountId, Balance, Block, Nonce, Runtime, RuntimeCall, RuntimeEvent};

use frame_support::{genesis_builder_helper::build_state, weights::Weight};
use pallet_grandpa::AuthorityId as GrandpaId;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, ConstU32, OpaqueMetadata};
use sp_runtime::{
	traits::{Block as BlockT, NumberFor},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, BoundedVec,
};
use sp_version::RuntimeVersion;

impl_runtime_apis! {
	impl runtime_apis::ElectoralRuntimeApi<Block, SolanaInstance> for Runtime {
		fn cf_electoral_data(account_id: AccountId) -> Vec<u8> {
			SolanaElections::electoral_data(&account_id).encode()
		}

		fn cf_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			SolanaElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}
	}

	// START custom runtime APIs
	impl runtime_apis::CustomRuntimeApi<Block> for Runtime {
		fn cf_is_auction_phase() -> bool {
			Validator::is_auction_phase()
		}
		fn cf_eth_flip_token_address() -> EvmAddress {
			Environment::supported_eth_assets(cf_primitives::chains::assets::eth::Asset::Flip).expect("FLIP token address should exist")
		}
		fn cf_eth_state_chain_gateway_address() -> EvmAddress {
			Environment::state_chain_gateway_address()
		}
		fn cf_eth_key_manager_address() -> EvmAddress {
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
			Emissions::backup_node_emission_per_block()
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
			let boost_pools_balances = IngressEgressBoostApi::boost_pool_account_balances(&account_id);
			free_balances.saturating_add(open_order_balances).saturating_add(boost_pools_balances)
		}
		fn cf_account_flip_balance(account_id: &AccountId) -> u128 {
			pallet_cf_flip::Account::<Runtime>::get(account_id).total()
		}
		fn cf_validator_info(account_id: &AccountId) -> ValidatorInfo {
			let is_current_backup = pallet_cf_validator::Backups::<Runtime>::get().contains_key(account_id);
			let key_holder_epochs = pallet_cf_validator::HistoricalActiveEpochs::<Runtime>::get(account_id);
			let is_qualified = <<Runtime as pallet_cf_validator::Config>::KeygenQualification as QualifyNode<_>>::is_qualified(account_id);
			let is_current_authority = pallet_cf_validator::CurrentAuthorities::<Runtime>::get().contains(account_id);
			let is_bidding = Validator::is_bidding(account_id);
			let bound_redeem_address = pallet_cf_funding::BoundRedeemAddress::<Runtime>::get(account_id);
			let apy_bp = calculate_account_apy(account_id);
			let reputation_info = pallet_cf_reputation::Reputations::<Runtime>::get(account_id);
			let account_info = pallet_cf_flip::Account::<Runtime>::get(account_id);
			let restricted_balances = pallet_cf_funding::RestrictedBalances::<Runtime>::get(account_id);
			let calculate_redeem_amount = pallet_cf_funding::Pallet::<Runtime>::calculate_redeem_amount(
				account_id,
				&restricted_balances,
				RedemptionAmount::Max,
				None,
			);
			ValidatorInfo {
				balance: account_info.total(),
				bond: account_info.bond(),
				last_heartbeat: pallet_cf_reputation::LastHeartbeat::<Runtime>::get(account_id).unwrap_or(0),
				reputation_points: reputation_info.reputation_points,
				keyholder_epochs: key_holder_epochs,
				is_current_authority,
				is_current_backup,
				is_qualified: is_bidding && is_qualified,
				is_online: HeartbeatQualification::<Runtime>::is_qualified(account_id),
				is_bidding,
				bound_redeem_address,
				apy_bp,
				restricted_balances,
				estimated_redeemable_balance: calculate_redeem_amount.redeem_amount,
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
			let min_active_bid = SetSizeMaximisingAuctionResolver::try_new(
				<Runtime as Chainflip>::EpochInfo::current_authority_count(),
				auction_params,
			)
			.and_then(|resolver| {
				resolver.resolve_auction(
					Validator::get_qualified_bidders::<<Runtime as pallet_cf_validator::Config>::KeygenQualification>(),
					Validator::auction_bid_cutoff_percentage(),
				)
			})
			.ok()
			.map(|auction_outcome| auction_outcome.bond);
			AuctionState {
				epoch_duration: Validator::epoch_duration(),
				current_epoch_started_at: Validator::current_epoch_started_at(),
				redemption_period_as_percentage: Validator::redemption_period_as_percentage().deconstruct(),
				min_funding: MinimumFunding::<Runtime>::get().unique_saturated_into(),
				auction_size_range: (auction_params.min_size, auction_params.max_size),
				min_active_bid,
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
		) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage> {
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
					pallet_cf_swapping::NetworkFee::<Runtime>::get(),
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
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_ingress_fee(asset)
					)
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_ingress_fee(asset)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_ingress_fee(asset).into()),
				ForeignChainAndAsset::Arbitrum(asset) => {
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_ingress_fee(asset)
					)
				},
				ForeignChainAndAsset::Solana(asset) => Some(SolanaChainTrackingProvider::estimate_ingress_fee(asset).into()),
				ForeignChainAndAsset::Assethub(asset) => {
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Assethub>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, AssethubInstance>::estimate_ingress_fee(asset)
					)
				},
			}
		}

		fn cf_egress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => {
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_egress_fee(asset)
					)
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_egress_fee(asset)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_egress_fee(asset).into()),
				ForeignChainAndAsset::Arbitrum(asset) => {
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_egress_fee(asset)
					)
				},
				ForeignChainAndAsset::Solana(asset) => Some(SolanaChainTrackingProvider::estimate_egress_fee(asset).into()),
				ForeignChainAndAsset::Assethub(asset) => {
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Assethub>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, AssethubInstance>::estimate_egress_fee(asset)
					)
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

			let current_block = System::block_number();

			pallet_cf_swapping::SwapQueue::<Runtime>::iter().flat_map(|(block, swaps_for_block)| {
				// In case `block` has already passed, the swaps will be re-tried at the next block:
				let execute_at = core::cmp::max(block, current_block.saturating_add(1));

				let swaps: Vec<_> = swaps_for_block
					.iter()
					.filter(|swap| swap.from == base_asset || swap.to == base_asset)
					.cloned()
					.collect();

				Swapping::get_scheduled_swap_legs(swaps, base_asset)
					.into_iter()
					.map(move |swap| (swap, execute_at))
			}).collect()
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

			fn boost_pools_depth<I: 'static>() -> Vec<BoostPoolDepth>
				where Runtime: pallet_cf_ingress_egress::Config<I> {

				pallet_cf_ingress_egress::BoostPools::<Runtime, I>::iter().map(|(asset, tier, pool)|

					BoostPoolDepth {
						asset: asset.into(),
						tier,
						available_amount: pool.get_available_amount().into()
					}

				).collect()
			}

			ForeignChain::iter().flat_map(|chain| {
				match chain {
					ForeignChain::Ethereum => boost_pools_depth::<EthereumInstance>(),
					ForeignChain::Polkadot => boost_pools_depth::<PolkadotInstance>(),
					ForeignChain::Bitcoin => boost_pools_depth::<BitcoinInstance>(),
					ForeignChain::Arbitrum => boost_pools_depth::<ArbitrumInstance>(),
					ForeignChain::Solana => boost_pools_depth::<SolanaInstance>(),
					ForeignChain::Assethub => boost_pools_depth::<AssethubInstance>(),
				}
			}).collect()

		}

		fn cf_boost_pool_details(asset: Asset) -> BTreeMap<u16, BoostPoolDetails> {

			fn boost_pools_details<I: 'static>(asset: TargetChainAsset::<Runtime, I>) -> BTreeMap<u16, BoostPoolDetails>
				where Runtime: pallet_cf_ingress_egress::Config<I> {

				let network_fee_deduction_percent = pallet_cf_ingress_egress::NetworkFeeDeductionFromBoostPercent::<Runtime, I>::get();

				pallet_cf_ingress_egress::BoostPools::<Runtime, I>::iter_prefix(asset).map(|(tier, pool)| {
					(
						tier,
						BoostPoolDetails {
							available_amounts: pool.get_amounts().into_iter().map(|(id, amount)| (id, amount.into())).collect(),
							pending_boosts: pool.get_pending_boosts().into_iter().map(|(deposit_id, owed_amounts)| {
								(
									deposit_id,
									owed_amounts.into_iter().map(|(id, amount)| (id, OwedAmount {total: amount.total.into(), fee: amount.fee.into()})).collect()
								)
							}).collect(),
							pending_withdrawals: pool.get_pending_withdrawals().clone(),
							network_fee_deduction_percent,
						}
					)
				}).collect()

			}

			let chain: ForeignChain = asset.into();

			match chain {
				ForeignChain::Ethereum => boost_pools_details::<EthereumInstance>(asset.try_into().unwrap()),
				ForeignChain::Polkadot => boost_pools_details::<PolkadotInstance>(asset.try_into().unwrap()),
				ForeignChain::Bitcoin => boost_pools_details::<BitcoinInstance>(asset.try_into().unwrap()),
				ForeignChain::Arbitrum => boost_pools_details::<ArbitrumInstance>(asset.try_into().unwrap()),
				ForeignChain::Solana => boost_pools_details::<SolanaInstance>(asset.try_into().unwrap()),
				ForeignChain::Assethub => boost_pools_details::<AssethubInstance>(asset.try_into().unwrap()),
			}

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

		fn cf_validate_refund_params(retry_duration: BlockNumber) -> Result<(), DispatchErrorWithMessage> {
			pallet_cf_swapping::Pallet::<Runtime>::validate_refund_params(retry_duration).map_err(Into::into)
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

			let retry_duration = match &extra_parameters {
				VaultSwapExtraParametersEncoded::Bitcoin { retry_duration, .. } => *retry_duration,
				VaultSwapExtraParametersEncoded::Ethereum(extra_params) => extra_params.refund_parameters.retry_duration,
				VaultSwapExtraParametersEncoded::Arbitrum(extra_params) => extra_params.refund_parameters.retry_duration,
				VaultSwapExtraParametersEncoded::Solana { refund_parameters, .. } => refund_parameters.retry_duration,
			};

			crate::chainflip::vault_swaps::validate_parameters(
				&broker,
				source_chain,
				&destination_address,
				destination_asset,
				&dca_parameters,
				boost_fee,
				broker_commission,
				&affiliate_fees,
				retry_duration,
				&channel_metadata,
			)?;

			// Validate parameters.
			if let Some(params) = dca_parameters.as_ref() {
				pallet_cf_swapping::Pallet::<Runtime>::validate_dca_params(params)?;
			}
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

			// Validate refund duration.
			pallet_cf_swapping::Pallet::<Runtime>::validate_refund_params(match &extra_parameters {
				VaultSwapExtraParametersEncoded::Bitcoin { retry_duration, .. } => *retry_duration,
				VaultSwapExtraParametersEncoded::Ethereum(extra_params) => extra_params.refund_parameters.retry_duration,
				VaultSwapExtraParametersEncoded::Arbitrum(extra_params) => extra_params.refund_parameters.retry_duration,
				VaultSwapExtraParametersEncoded::Solana { refund_parameters, .. } => refund_parameters.retry_duration,
			})?;

			// Validate CCM.
			if let Some(channel_metadata) = channel_metadata.as_ref() {
				if source_chain == ForeignChain::Bitcoin {
					return Err(DispatchErrorWithMessage::from("Vault swaps with CCM are not supported for the Bitcoin Chain"));
				}
				if !destination_chain.ccm_support() {
					return Err(DispatchErrorWithMessage::from("Destination chain does not support CCM"));
				}

				// Ensure CCM message is valid
				match channel_metadata.clone().to_checked(
					destination_asset,
					ChainAddressConverter::try_from_encoded_address(destination_address.clone())
						.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDestinationAddress)?
				).map(|checked| checked.ccm_additional_data)
				{
					Ok(DecodedCcmAdditionalData::Solana(decoded)) => {
						let ccm_accounts = decoded.ccm_accounts();

						// Ensure the CCM parameters do not contain blacklisted accounts.
						// Load up environment variables.
						let api_environment =
							SolEnvironment::api_environment().map_err(|_| "Failed to load Solana API environment")?;

						let agg_key: SolPubkey = SolEnvironment::current_agg_key()
							.map_err(|_| "Failed to load Solana Agg key")?
							.into();

						let on_chain_key: SolPubkey = SolEnvironment::current_on_chain_key()
							.map(|key| key.into())
							.unwrap_or_else(|_| agg_key);

						check_ccm_for_blacklisted_accounts(
							&ccm_accounts,
							vec![api_environment.token_vault_pda_account.into(), agg_key, on_chain_key],
						)
						.map_err(DispatchError::from)?;
					},
					Ok(DecodedCcmAdditionalData::NotRequired) => {},
					Err(_) => return Err(DispatchErrorWithMessage::from("Solana Ccm additional data is invalid")),
				};
			}

			// Encode swap
			match (source_chain, extra_parameters) {
				(
					ForeignChain::Bitcoin,
					VaultSwapExtraParameters::Bitcoin {
						min_output_amount,
						retry_duration,
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
						channel_metadata,
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
					channel_metadata,
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
			refund_parameters: ChannelRefundParametersEncoded,
			dca_parameters: Option<DcaParameters>,
			boost_fee: BasisPoints,
			broker_commission: BasisPoints,
			affiliate_fees: Affiliates<AccountId>,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
		) -> Result<Vec<u8>, DispatchErrorWithMessage> {
			// Validate the parameters
			crate::chainflip::vault_swaps::validate_parameters(
				&broker,
				source_asset.into(),
				&destination_address,
				destination_asset,
				&dca_parameters,
				boost_fee,
				broker_commission,
				&affiliate_fees,
				refund_parameters.retry_duration,
				&channel_metadata,
			)?;

			let boost_fee: u8 = boost_fee
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh)?;

			let affiliate_and_fees = crate::chainflip::vault_swaps::to_affiliate_and_fees(&broker, affiliate_fees)?
				.try_into()
				.map_err(|_| "Too many affiliates.")?;

			macro_rules! build_cf_parameters_for_chain {
				($chain:ty) => {
					build_cf_parameters::<$chain>(
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
						channel_metadata.as_ref(),
					)
				}
			}

			Ok(match ForeignChain::from(source_asset) {
				ForeignChain::Ethereum => build_cf_parameters_for_chain!(Ethereum),
				ForeignChain::Arbitrum => build_cf_parameters_for_chain!(Arbitrum),
				ForeignChain::Solana => build_cf_parameters_for_chain!(Solana),
				_ => Err(DispatchErrorWithMessage::from("Unsupported source chain for encoding cf_parameters"))?,
			})
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

		fn cf_all_open_deposit_channels() -> Vec<(AccountId, ChannelActionType, ChainAccounts)> {
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

		fn cf_trading_strategy_limits() -> TradingStrategyLimits {
			TradingStrategyLimits{
				minimum_deployment_amount: AssetMap::from_iter(pallet_cf_trading_strategy::MinimumDeploymentAmountForStrategy::<Runtime>::get().into_iter()
					.map(|(asset, balance)| (asset, Some(balance)))),
				minimum_added_funds_amount: AssetMap::from_iter(pallet_cf_trading_strategy::MinimumAddedFundsToStrategy::<Runtime>::get().into_iter()
					.map(|(asset, balance)| (asset, Some(balance)))),
			}
		}

		fn cf_network_fees() -> NetworkFees {
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
	}


	impl monitoring_apis::MonitoringRuntimeApi<Block> for Runtime {

		fn cf_authorities() -> AuthoritiesInfo {
			let mut authorities = pallet_cf_validator::CurrentAuthorities::<Runtime>::get();
			let mut backups = pallet_cf_validator::Backups::<Runtime>::get();
			let mut result = AuthoritiesInfo {
				authorities: authorities.len() as u32,
				online_authorities: 0,
				backups: backups.len() as u32,
				online_backups: 0,
			};
			authorities.retain(HeartbeatQualification::<Runtime>::is_qualified);
			backups.retain(|id, _| HeartbeatQualification::<Runtime>::is_qualified(id));
			result.online_authorities = authorities.len() as u32;
			result.online_backups = backups.len() as u32;
			result
		}

		fn cf_external_chains_block_height() -> ExternalChainsBlockHeight {
			// safe to unwrap these value as stated on the storage item doc
			let btc = pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::get().unwrap();
			let eth = pallet_cf_chain_tracking::CurrentChainState::<Runtime, EthereumInstance>::get().unwrap();
			let dot = pallet_cf_chain_tracking::CurrentChainState::<Runtime, PolkadotInstance>::get().unwrap();
			let arb = pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::get().unwrap();
			let sol = SolanaChainTrackingProvider::get_block_height();
			let hub = pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::get().unwrap();

			ExternalChainsBlockHeight {
				bitcoin: btc.block_height,
				ethereum: eth.block_height,
				polkadot: dot.block_height.into(),
				solana: sol,
				arbitrum: arb.block_height,
				assethub: hub.block_height.into(),
			}
		}

		fn cf_btc_utxos() -> BtcUtxos {
			let utxos = pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get();
			let mut btc_balance = utxos.iter().fold(0, |acc, elem| acc + elem.amount);
			//Sum the btc balance contained in the change utxos to the btc "free_balance"
			let btc_ceremonies = pallet_cf_threshold_signature::PendingCeremonies::<Runtime,BitcoinInstance>::iter_values().map(|ceremony|{
				ceremony.request_context.request_id
			}).collect::<Vec<_>>();
			let EpochKey { key, .. } = pallet_cf_threshold_signature::Pallet::<Runtime, BitcoinInstance>::active_epoch_key()
				.expect("We should always have a key for the current epoch");
			for ceremony in btc_ceremonies {
				if let RuntimeCall::BitcoinBroadcaster(pallet_cf_broadcast::pallet::Call::on_signature_ready{ api_call, ..}) = pallet_cf_threshold_signature::RequestCallback::<Runtime, BitcoinInstance>::get(ceremony).unwrap() {
					if let BitcoinApi::BatchTransfer(batch_transfer) = *api_call {
						for output in batch_transfer.bitcoin_transaction.outputs {
							if [
								ScriptPubkey::Taproot(key.previous.unwrap_or_default()),
								ScriptPubkey::Taproot(key.current),
							]
							.contains(&output.script_pubkey)
							{
								btc_balance += output.amount;
							}
						}
					}
				}
			}
			BtcUtxos {
				total_balance: btc_balance,
				count: utxos.len() as u32,
			}
		}

		fn cf_dot_aggkey() -> PolkadotAccountId {
			let epoch = PolkadotThresholdSigner::current_key_epoch().unwrap_or_default();
			PolkadotThresholdSigner::keys(epoch).unwrap_or_default()
		}

		fn cf_suspended_validators() -> Vec<(Offence, u32)> {
			let suspended_for_keygen = match pallet_cf_validator::Pallet::<Runtime>::current_rotation_phase() {
				pallet_cf_validator::RotationPhase::KeygensInProgress(rotation_state) |
				pallet_cf_validator::RotationPhase::KeyHandoversInProgress(rotation_state) |
				pallet_cf_validator::RotationPhase::ActivatingKeys(rotation_state) |
				pallet_cf_validator::RotationPhase::NewKeysActivated(rotation_state) => { rotation_state.banned.len() as u32 },
				_ => {0u32}
			};
			pallet_cf_reputation::Suspensions::<Runtime>::iter().map(|(key, _)| {
				if key == pallet_cf_threshold_signature::PalletOffence::FailedKeygen.into() {
					return (key, suspended_for_keygen);
				}
				(key, pallet_cf_reputation::Pallet::<Runtime>::validators_suspended_for(&[key]).len() as u32)
			}).collect()
		}
		fn cf_epoch_state() -> EpochState {
			let auction_params = Validator::auction_parameters();
			let min_active_bid = SetSizeMaximisingAuctionResolver::try_new(
				<Runtime as Chainflip>::EpochInfo::current_authority_count(),
				auction_params,
			)
			.and_then(|resolver| {
				resolver.resolve_auction(
					Validator::get_qualified_bidders::<<Runtime as pallet_cf_validator::Config>::KeygenQualification>(),
					Validator::auction_bid_cutoff_percentage(),
				)
			})
			.ok()
			.map(|auction_outcome| auction_outcome.bond);
			EpochState {
				epoch_duration: Validator::epoch_duration(),
				current_epoch_started_at: Validator::current_epoch_started_at(),
				current_epoch_index: Validator::current_epoch(),
				min_active_bid,
				rotation_phase: Validator::current_rotation_phase().to_str().to_string(),
			}
		}
		fn cf_redemptions() -> RedemptionsInfo {
			let redemptions: Vec<_> = pallet_cf_funding::PendingRedemptions::<Runtime>::iter().collect();
			RedemptionsInfo {
				total_balance: redemptions.iter().fold(0, |acc, elem| acc + elem.1.total),
				count: redemptions.len() as u32,
			}
		}
		fn cf_pending_broadcasts_count() -> PendingBroadcasts {
			PendingBroadcasts {
				ethereum: pallet_cf_broadcast::PendingBroadcasts::<Runtime, EthereumInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				bitcoin: pallet_cf_broadcast::PendingBroadcasts::<Runtime, BitcoinInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				polkadot: pallet_cf_broadcast::PendingBroadcasts::<Runtime, PolkadotInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				arbitrum: pallet_cf_broadcast::PendingBroadcasts::<Runtime, ArbitrumInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				solana: pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				assethub: pallet_cf_broadcast::PendingBroadcasts::<Runtime, AssethubInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
			}
		}
		fn cf_pending_tss_ceremonies_count() -> PendingTssCeremonies {
			PendingTssCeremonies {
				evm: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, EvmInstance>::iter().collect::<Vec<_>>().len() as u32,
				bitcoin: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, BitcoinInstance>::iter().collect::<Vec<_>>().len() as u32,
				polkadot: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, PolkadotCryptoInstance>::iter().collect::<Vec<_>>().len() as u32,
				solana: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, SolanaInstance>::iter().collect::<Vec<_>>().len() as u32,
			}
		}
		fn cf_pending_swaps_count() -> u32 {
			let swaps: Vec<_> = pallet_cf_swapping::SwapQueue::<Runtime>::iter().collect();
			swaps.iter().fold(0u32, |acc, elem| acc + elem.1.len() as u32)
		}
		fn cf_open_deposit_channels_count() -> OpenDepositChannels {
			fn open_channels<BlockHeight, I: 'static>() -> u32
				where BlockHeight: GetBlockHeight<<Runtime as pallet_cf_ingress_egress::Config<I>>::TargetChain>, Runtime: pallet_cf_ingress_egress::Config<I>
			{
				pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, I>::iter().filter(|(_key, elem)| elem.expires_at > BlockHeight::get_block_height()).collect::<Vec<_>>().len() as u32
			}

			OpenDepositChannels{
				ethereum: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, EthereumInstance>, EthereumInstance>(),
				bitcoin: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, BitcoinInstance>, BitcoinInstance>(),
				polkadot: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, PolkadotInstance>, PolkadotInstance>(),
				arbitrum: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, ArbitrumInstance>, ArbitrumInstance>(),
				solana: open_channels::<SolanaChainTrackingProvider, SolanaInstance>(),
				assethub: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, AssethubInstance>, AssethubInstance>(),
			}
		}
		fn cf_fee_imbalance() -> FeeImbalance<AssetAmount> {
			FeeImbalance {
				ethereum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Ethereum.gas_asset()),
				polkadot: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Polkadot.gas_asset()),
				arbitrum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Arbitrum.gas_asset()),
				bitcoin: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Bitcoin.gas_asset()),
				solana: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Solana.gas_asset()),
				assethub: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Assethub.gas_asset()),
			}
		}
		fn cf_build_version() -> LastRuntimeUpgradeInfo {
			let info = frame_system::LastRuntimeUpgrade::<Runtime>::get().expect("this has to be set");
			LastRuntimeUpgradeInfo {
				spec_version: info.spec_version.into(),
				spec_name: info.spec_name.to_string(),
			}
		}
		fn cf_rotation_broadcast_ids() -> ActivateKeysBroadcastIds{
			ActivateKeysBroadcastIds{
				ethereum: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, EthereumInstance>::get().map(|val| val.1),
				bitcoin: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, BitcoinInstance>::get().map(|val| val.1),
				polkadot: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, PolkadotInstance>::get().map(|val| val.1),
				arbitrum: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, ArbitrumInstance>::get().map(|val| val.1),
				solana: {
					let broadcast_id = pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, SolanaInstance>::get().map(|val| val.1);
					(broadcast_id, pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::get(broadcast_id.unwrap_or_default()).map(|broadcast_data| broadcast_data.transaction_out_id))
				},
				assethub: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, AssethubInstance>::get().map(|val| val.1),
			}
		}
		fn cf_sol_nonces() -> SolanaNonces{
			SolanaNonces {
				available: pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::get(),
				unavailable: pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter_keys().collect()
			}
		}
		fn cf_sol_aggkey() -> SolAddress {
			let epoch = SolanaThresholdSigner::current_key_epoch().unwrap_or_default();
			SolanaThresholdSigner::keys(epoch).unwrap_or_default()
		}
		fn cf_sol_onchain_key() -> SolAddress {
			SolanaBroadcaster::current_on_chain_key().unwrap_or_default()
		}
		fn cf_monitoring_data() -> MonitoringDataV2 {
			MonitoringDataV2 {
				external_chains_height: Self::cf_external_chains_block_height(),
				btc_utxos: Self::cf_btc_utxos(),
				epoch: Self::cf_epoch_state(),
				pending_redemptions: Self::cf_redemptions(),
				pending_broadcasts: Self::cf_pending_broadcasts_count(),
				pending_tss: Self::cf_pending_tss_ceremonies_count(),
				open_deposit_channels: Self::cf_open_deposit_channels_count(),
				fee_imbalance: Self::cf_fee_imbalance(),
				authorities: Self::cf_authorities(),
				build_version: Self::cf_build_version(),
				suspended_validators: Self::cf_suspended_validators(),
				pending_swaps: Self::cf_pending_swaps_count(),
				dot_aggkey: Self::cf_dot_aggkey(),
				flip_supply: {
					let flip = Self::cf_flip_supply();
					FlipSupply { total_supply: flip.0, offchain_supply: flip.1}
				},
				sol_aggkey: Self::cf_sol_aggkey(),
				sol_onchain_key: Self::cf_sol_onchain_key(),
				sol_nonces: Self::cf_sol_nonces(),
				activating_key_broadcast_ids: Self::cf_rotation_broadcast_ids(),
			}
		}
		fn cf_accounts_info(accounts: BoundedVec<AccountId, ConstU32<10>>) -> Vec<ValidatorInfo> {
			accounts.iter().map(|account_id| {
				Self::cf_validator_info(account_id)
			}).collect()
		}
	}

	// END custom runtime APIs
	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			crate::VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block);
		}

		fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> sp_std::vec::Vec<u32> {
			Runtime::metadata_versions()
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
		}

		fn authorities() -> Vec<AuraId> {
			pallet_aura::Authorities::<Runtime>::get().into_inner()
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
			opaque::SessionKeys::generate(seed)
		}

		fn decode_session_keys(
			encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
			opaque::SessionKeys::decode_into_raw_public_keys(&encoded)
		}
	}


	impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
		fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
			Grandpa::grandpa_authorities()
		}

		fn current_set_id() -> sp_consensus_grandpa::SetId {
			Grandpa::current_set_id()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: sp_consensus_grandpa::EquivocationProof<
				<Block as BlockT>::Hash,
				NumberFor<Block>,
			>,
			key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			let key_owner_proof = key_owner_proof.decode()?;

			Grandpa::submit_unsigned_equivocation_report(
				equivocation_proof,
				key_owner_proof,
			)
		}

		fn generate_key_ownership_proof(
			_set_id: sp_consensus_grandpa::SetId,
			authority_id: GrandpaId,
		) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
			use frame_support::traits::KeyOwnerProofSystem;
			Historical::prove((sp_consensus_grandpa::KEY_TYPE, authority_id))
				.map(|p| p.encode())
				.map(sp_consensus_grandpa::OpaqueKeyOwnershipProof::new)
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(account: AccountId) -> Nonce {
			System::account_nonce(account)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
		fn query_info(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_info(uxt, len)
		}
		fn query_fee_details(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall>
		for Runtime
	{
		fn query_call_info(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_call_info(call, len)
		}
		fn query_call_fee_details(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_call_fee_details(call, len)
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here. If any of the pre/post migration checks fail, we shall stop
			// right here and right now.
			let weight = Executive::try_runtime_upgrade(checks)
				.inspect_err(|e| log::error!("try_runtime_upgrade failed with: {:?}", e)).unwrap();
			(weight, BlockWeights::get().max_block)
		}

		fn execute_block(
			block: Block,
			state_root_check: bool,
			signature_check: bool,
			select: frame_try_runtime::TryStateSelect
		) -> Weight {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here.
			Executive::try_execute_block(block, state_root_check, signature_check, select).expect("execute-block failed")
		}
	}

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn get_preset(_id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
			None
		}

		fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
			Default::default()
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
		) {
			use frame_benchmarking::{baseline, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;
			use frame_system_benchmarking::extensions::Pallet as SystemExtensionsBench;
			use cf_session_benchmarking::Pallet as SessionBench;
			use baseline::Pallet as BaselineBench;
			use super::*;

			let mut list = Vec::<BenchmarkList>::new();

			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();

			(list, storage_info)
		}

		#[allow(non_local_definitions)]
		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, core::alloc::string::String> {
			use frame_benchmarking::{baseline, BenchmarkBatch};
			use sp_storage::TrackedStorageKey;
			use frame_system_benchmarking::Pallet as SystemBench;
			use frame_system_benchmarking::extensions::Pallet as SystemExtensionsBench;
			use cf_session_benchmarking::Pallet as SessionBench;
			use baseline::Pallet as BaselineBench;
			use super::*;

			impl cf_session_benchmarking::Config for Runtime {}
			impl frame_system_benchmarking::Config for Runtime {}
			impl baseline::Config for Runtime {}

			use frame_support::traits::WhitelistedStorageKeys;
			let whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);
			add_benchmarks!(params, batches);

			Ok(batches)
		}
	}
}
