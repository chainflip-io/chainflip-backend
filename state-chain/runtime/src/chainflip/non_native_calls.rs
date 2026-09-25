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

use crate::RuntimeCall;
use frame_support::traits::Contains;
use pallet_cf_account_roles::Call as AccountRolesCall;
use pallet_cf_environment::Call as EnvironmentCall;

/// EIP-712 signing, like the other signature types of `non_native_signed_call`, is restricted so
/// that only the calls allowed here can be submitted. Anything else is rejected before its signed
/// payload is built.
pub struct AllowedNonNativeCalls;

impl Contains<RuntimeCall> for AllowedNonNativeCalls {
	fn contains(call: &RuntimeCall) -> bool {
		is_allowed(call, false, false)
	}
}

/// `batch` and `as_sub_account` may each wrap a call at most once, e.g. a batch of sub-account
/// calls. A batch never contains another batch, however deeply it is wrapped.
fn is_allowed(call: &RuntimeCall, in_batch: bool, in_sub_account: bool) -> bool {
	match call {
		RuntimeCall::Environment(EnvironmentCall::batch { calls }) =>
			!in_batch && calls.iter().all(|call| is_allowed(call, true, in_sub_account)),
		RuntimeCall::AccountRoles(AccountRolesCall::as_sub_account { call, .. }) =>
			!in_sub_account && is_allowed(call, in_batch, true),
		call => is_listed(call),
	}
}

/// Generates `is_listed`, an exhaustive match in which every pallet is either denied or lists each
/// of its calls as allowed or denied. Adding a call or pallet therefore fails to compile until it
/// is listed here (hence no catch-all arm). Instances of one pallet share an entry. Entries take
/// attributes, so calls that only exist with some features can be `#[cfg]`-gated to match.
macro_rules! classify_calls {
	(@calls $call:ident, $pallet_crate:ident,
		[$($(#[$allowed_attr:meta])* $allowed:ident),* $(,)?],
		[$($(#[$denied_attr:meta])* $denied:ident),* $(,)?]
	) => {
		match $call {
			$($(#[$allowed_attr])* $pallet_crate::Call::$allowed { .. } => true,)*
			$($(#[$denied_attr])* $pallet_crate::Call::$denied { .. } => false,)*
			$pallet_crate::Call::__Ignore(..) => false,
		}
	};
	(
		$(
			$($pallet:ident),+ ($pallet_crate:ident) {
				allowed: $allowed:tt,
				denied: $denied:tt $(,)?
			}
		),* $(,)?
		;
		denied_pallets: [$($(#[$denied_pallet_attr:meta])* $denied_pallet:ident),* $(,)?] $(,)?
	) => {
		fn is_listed(call: &RuntimeCall) -> bool {
			match call {
				$($(
					RuntimeCall::$pallet(call) =>
						classify_calls!(@calls call, $pallet_crate, $allowed, $denied),
				)+)*
				$($(#[$denied_pallet_attr])* RuntimeCall::$denied_pallet(_) => false,)*
			}
		}
	};
}

// Only the calls allowed here can be submitted with EIP-712 or another non-native signature. Some
// pallets are denied as a whole; every other pallet lists each of its calls as allowed or denied.
// To allow a call in a denied pallet, list that pallet's calls individually instead.
//
// Allow only the small calls that are signed with external wallets in practice, mostly by LPs,
// delegators and operators, and deny the rest. `MAX_NON_NATIVE_CALL_SIZE` is sized for the allowed
// calls. `batch` and `as_sub_account` are allowed as wrappers: `is_allowed` checks the calls they
// wrap.
classify_calls! {
	AccountRoles(pallet_cf_account_roles) {
		allowed: [
			set_vanity_name,
			as_sub_account,
		],
		denied: [
			spawn_sub_account,
		],
	},
	AssetBalances(pallet_cf_asset_balances) {
		allowed: [
			update_whitelist,
			set_whitelist_timelock,
		],
		denied: [
			update_pallet_config,
		],
	},
	Environment(pallet_cf_environment) {
		allowed: [
			batch,
		],
		denied: [
			witness_polkadot_vault_creation,
			witness_current_bitcoin_block_number_for_key,
			update_safe_mode,
			update_consolidation_parameters,
			witness_initialize_arbitrum_vault,
			witness_initialize_solana_vault,
			force_recover_sol_nonce,
			dispatch_solana_gov_call,
			witness_assethub_vault_creation,
			non_native_signed_call,
			witness_initialize_tron_vault,
			witness_initialize_bsc_vault,
			submit_elections_votes,
			#[cfg(feature = "runtime-benchmarks")]
			benchmark_realistic_call,
			update_pallet_config,
		],
	},
	Funding(pallet_cf_funding) {
		allowed: [
			redeem,
			bind_redeem_address,
			bind_executor_address,
			rebalance,
		],
		denied: [
			update_minimum_funding,
			update_restricted_addresses,
			update_redemption_tax,
			execute_sc_call,
		],
	},
	LendingPools(pallet_cf_lending_pools) {
		allowed: [
			add_boost_funds,
			stop_boosting,
			add_lender_funds,
			remove_lender_funds,
			request_loan,
			expand_loan,
			make_repayment,
			initiate_voluntary_liquidation,
			stop_voluntary_liquidation,
		],
		denied: [
			update_pallet_config,
			create_boost_pools,
			create_lending_pool,
		],
	},
	LiquidityPools(pallet_cf_pools) {
		allowed: [
			set_limit_order,
			update_limit_order,
			set_range_order,
			update_range_order,
		],
		denied: [
			new_pool,
			set_pool_fees,
			set_maximum_price_impact,
			update_pallet_config,
			cancel_orders_batch,
		],
	},
	LiquidityProvider(pallet_cf_lp) {
		allowed: [
			register_lp_account,
			deregister_lp_account,
			register_liquidity_refund_address,
			request_liquidity_deposit_address,
			withdraw_asset,
			transfer_asset,
			schedule_swap,
			transfer_flip_to_on_chain_balance,
		],
		denied: [
			purge_balances,
		],
	},
	Swapping(pallet_cf_swapping) {
		allowed: [
			request_account_creation_deposit_address,
		],
		denied: [
			register_as_broker,
			deregister_as_broker,
			request_swap_deposit_address,
			request_swap_deposit_address_with_affiliates,
			withdraw,
			open_private_btc_channel,
			close_private_btc_channel,
			register_affiliate,
			deregister_affiliate,
			affiliate_withdrawal_request,
			set_vault_swap_minimum_broker_fee,
			bind_broker_fee_withdrawal_address,
			update_pallet_config,
		],
	},
	TradingStrategy(pallet_cf_trading_strategy) {
		allowed: [
			deploy_strategy,
			close_strategy,
			add_funds_to_strategy,
		],
		denied: [
			update_pallet_config,
		],
	},
	Validator(pallet_cf_validator) {
		allowed: [
			delegate,
			undelegate,
			register_as_operator,
			deregister_as_operator,
			update_operator_settings,
			claim_validator,
			remove_validator,
			block_delegator,
			allow_delegator,
		],
		denied: [
			update_pallet_config,
			force_rotation,
			set_keys,
			register_peer_id,
			cfe_version,
			register_as_validator,
			deregister_as_validator,
			start_bidding,
			stop_bidding,
			set_validator_max_bid,
			accept_operator,
			report_witnessing_task_restart,
			delegate_grandpa_vote,
			revoke_grandpa_delegation,
		],
	},
	;
	denied_pallets: [
		System,
		Timestamp,
		Flip,
		Emissions,
		Witnesser,
		Session,
		Grandpa,
		Governance,
		Reputation,
		TokenholderGovernance,
		EthereumChainTracking,
		PolkadotChainTracking,
		BitcoinChainTracking,
		EthereumVault,
		PolkadotVault,
		BitcoinVault,
		EvmThresholdSigner,
		PolkadotThresholdSigner,
		BitcoinThresholdSigner,
		EthereumBroadcaster,
		PolkadotBroadcaster,
		BitcoinBroadcaster,
		ArbitrumChainTracking,
		ArbitrumVault,
		ArbitrumBroadcaster,
		SolanaVault,
		SolanaThresholdSigner,
		SolanaBroadcaster,
		SolanaElections,
		SolanaChainTracking,
		AssethubChainTracking,
		AssethubVault,
		AssethubBroadcaster,
		BitcoinElections,
		GenericElections,
		EthereumElections,
		ArbitrumElections,
		TronChainTracking,
		TronVault,
		TronBroadcaster,
		TronElections,
		BscChainTracking,
		BscVault,
		BscBroadcaster,
		BscElections,
		AssethubElections,
		EthereumIngressEgress,
		PolkadotIngressEgress,
		BitcoinIngressEgress,
		ArbitrumIngressEgress,
		SolanaIngressEgress,
		AssethubIngressEgress,
		TronIngressEgress,
		BscIngressEgress,
	],
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_cf_lp::Call as LpCall;
	use pallet_cf_pools::Call as PoolsCall;
	use pallet_cf_trading_strategy::Call as TradingStrategyCall;
	use pallet_cf_validator::Call as ValidatorCall;

	fn allowed_call() -> RuntimeCall {
		LpCall::register_lp_account {}.into()
	}

	fn disallowed_call() -> RuntimeCall {
		LpCall::purge_balances { accounts: Default::default() }.into()
	}

	fn batch(calls: Vec<RuntimeCall>) -> RuntimeCall {
		EnvironmentCall::batch { calls: calls.try_into().unwrap() }.into()
	}

	fn as_sub_account(call: RuntimeCall) -> RuntimeCall {
		AccountRolesCall::as_sub_account { sub_account_index: 0, call: Box::new(call) }.into()
	}

	#[test]
	fn only_listed_calls_are_allowed() {
		assert!(AllowedNonNativeCalls::contains(&allowed_call()));
		assert!(!AllowedNonNativeCalls::contains(&disallowed_call()));
		assert!(!AllowedNonNativeCalls::contains(
			&frame_system::Call::remark { remark: Default::default() }.into()
		));
		// Operators manage their delegation settings with EIP-712 too, but validators do not.
		assert!(AllowedNonNativeCalls::contains(&ValidatorCall::deregister_as_operator {}.into()));
		assert!(!AllowedNonNativeCalls::contains(
			&ValidatorCall::accept_operator { operator: [0; 32].into() }.into()
		));
	}

	#[test]
	fn wrapped_calls_must_all_be_allowed() {
		assert!(AllowedNonNativeCalls::contains(&batch(vec![allowed_call(), allowed_call()])));
		assert!(!AllowedNonNativeCalls::contains(&batch(vec![allowed_call(), disallowed_call()])));
		assert!(AllowedNonNativeCalls::contains(&as_sub_account(allowed_call())));
		assert!(!AllowedNonNativeCalls::contains(&as_sub_account(disallowed_call())));
		assert!(AllowedNonNativeCalls::contains(&as_sub_account(batch(vec![allowed_call()]))));
		assert!(AllowedNonNativeCalls::contains(&batch(vec![as_sub_account(allowed_call())])));
	}

	#[test]
	fn a_batch_never_contains_a_batch() {
		let nested = batch(vec![allowed_call()]);
		assert!(!AllowedNonNativeCalls::contains(&batch(vec![nested.clone()])));
		assert!(!AllowedNonNativeCalls::contains(&batch(vec![as_sub_account(nested.clone())])));
		assert!(!AllowedNonNativeCalls::contains(&as_sub_account(batch(vec![nested]))));
	}

	#[test]
	fn sub_account_calls_are_not_nested() {
		assert!(!AllowedNonNativeCalls::contains(&as_sub_account(as_sub_account(allowed_call()))));
		assert!(!AllowedNonNativeCalls::contains(&as_sub_account(batch(vec![as_sub_account(
			allowed_call()
		)]))));
	}

	/// Allowed calls must fit comfortably, leaving room to batch them.
	#[test]
	fn common_calls_fit_the_size_cap() {
		use cf_amm::common::Side;
		use cf_primitives::Asset;
		use codec::Encode;
		use pallet_cf_environment::{MAX_BATCHED_CALLS, MAX_NON_NATIVE_CALL_SIZE};

		let fully_funded_strategy: RuntimeCall = TradingStrategyCall::deploy_strategy {
			strategy: pallet_cf_trading_strategy::TradingStrategy::OracleTracking {
				min_buy_offset_tick: 0,
				max_buy_offset_tick: 0,
				min_sell_offset_tick: 0,
				max_sell_offset_tick: 0,
				base_asset: Asset::Btc,
				quote_asset: Asset::Usdc,
			},
			funding: Asset::all().map(|asset| (asset, u128::MAX)).collect(),
		}
		.into();
		assert!(AllowedNonNativeCalls::contains(&fully_funded_strategy));
		// Leaves room to batch it with other calls.
		assert!(fully_funded_strategy.encoded_size() * 2 <= MAX_NON_NATIVE_CALL_SIZE);

		let limit_order: RuntimeCall = PoolsCall::set_limit_order {
			base_asset: Asset::Btc,
			quote_asset: Asset::Usdc,
			side: Side::Buy,
			id: u64::MAX,
			option_tick: Some(0),
			sell_amount: u128::MAX,
			dispatch_at: Some(u32::MAX),
			close_order_at: Some(u32::MAX),
		}
		.into();
		let full_batch = batch(vec![limit_order; MAX_BATCHED_CALLS as usize]);
		assert!(AllowedNonNativeCalls::contains(&full_batch));
		assert!(full_batch.encoded_size() <= MAX_NON_NATIVE_CALL_SIZE);
	}
}
