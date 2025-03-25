use cf_primitives::{AccountRole, Asset, AuthorityCount, FLIPPERINOS_PER_FLIP};
use cf_traits::{FeeScalingRateConfig, IncreaseOrDecrease};
use frame_support::pallet_prelude::{InvalidTransaction, TransactionValidityError};
use pallet_cf_flip::FeeScalingRate;
use pallet_cf_pools::RangeOrderSize;
use sp_keyring::Ed25519Keyring as AccountKeyring;
use sp_runtime::FixedU64;
use state_chain_runtime::{Flip, Runtime, RuntimeCall};

use crate::network::apply_extrinsic_and_calculate_gas_fee;

fn update_range_order_call(base_asset: Asset) -> RuntimeCall {
	RuntimeCall::LiquidityPools(pallet_cf_pools::Call::update_range_order {
		base_asset,
		quote_asset: Asset::Usdc,
		id: 0,
		option_tick_range: Some(-10..10),
		size_change: IncreaseOrDecrease::Decrease(RangeOrderSize::Liquidity {
			liquidity: 1_000u128,
		}),
	})
}

use cf_traits::TransactionFeeScaler;

#[test]
fn fee_scales_within_a_pool() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	let lp = AccountKeyring::Alice;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[(
			lp.to_account_id(),
			AccountRole::LiquidityProvider,
			5 * FLIPPERINOS_PER_FLIP,
		)])
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) =
				crate::network::fund_authorities_and_join_auction(MAX_AUTHORITIES);

			let lp_account_id = lp.to_account_id();

			let call = update_range_order_call(Asset::Eth);

			let (_call_info_id, upfront_fee) = <Runtime as pallet_cf_flip::Config>::TransactionFeeScaler::call_info_and_spam_prevention_upfront_fee(
				&call,
				&lp_account_id,
			).expect("We chose a call that should be scaled.");

			assert_eq!(FeeScalingRate::<Runtime>::get(), FeeScalingRateConfig::NoScaling);

			let (mut last_gas, mut last_remaining_balance) =
				apply_extrinsic_and_calculate_gas_fee(lp, call.clone()).unwrap();
			(0..50).for_each(|_| {
				let (gas, remaining_balance) =
					apply_extrinsic_and_calculate_gas_fee(lp, call.clone()).unwrap();
				assert_ne!(gas, 0);
				assert_eq!(gas, last_gas);
				last_gas = gas;
				assert!(remaining_balance < last_remaining_balance);
				last_remaining_balance = remaining_balance;
			});

			// Reset the fee scaling counters
			testnet.move_forward_blocks(1);

			// Set the config to scale per pool.
			FeeScalingRate::<Runtime>::set(FeeScalingRateConfig::ExponentBuffer {
				buffer: 0,
				exp_base: FixedU64::from_rational(2, 1u128),
			});

			let (mut last_gas, mut last_remaining_balance) =
				apply_extrinsic_and_calculate_gas_fee(lp, call.clone()).unwrap();

			let mut can_pay = true;
			(1..50).for_each(|_| {
				match apply_extrinsic_and_calculate_gas_fee(lp, call.clone()) {
					Ok((gas, remaining_balance)) => {
						// Either the gas is increasnig, or we've hit the ceiling of the upfront fee.
						assert!(gas > last_gas || gas == upfront_fee);
						assert_eq!(remaining_balance, last_remaining_balance - gas);
						// We should never fail, and then succeed again, since the fee always
						// increases within a block in the same pool.
						assert!(can_pay);
						last_remaining_balance = remaining_balance;
						last_gas = gas;
					},
					Err(e) => {
						can_pay = false;
						// We no longer have enough to pay fees.
						assert_eq!(
							e,
							TransactionValidityError::Invalid(InvalidTransaction::Payment)
						);
						// No balance change if transaction fails
						assert_eq!(Flip::total_balance_of(&lp_account_id), last_remaining_balance);
					},
				}
			});

			// We should have run out of balance by the end.
			assert!(!can_pay);
		});
}

#[test]
fn fee_scales_per_pool() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	let lp = AccountKeyring::Alice;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[(
			lp.to_account_id(),
			AccountRole::LiquidityProvider,
			5 * FLIPPERINOS_PER_FLIP,
		)])
		.build()
		.execute_with(|| {
			crate::network::fund_authorities_and_join_auction(MAX_AUTHORITIES);

			FeeScalingRate::<Runtime>::set(FeeScalingRateConfig::ExponentBuffer {
				buffer: 0,
				exp_base: FixedU64::from_rational(2, 1u128),
			});

			let call = update_range_order_call(Asset::Eth);
			let call2 = update_range_order_call(Asset::Btc);

			// Different pools so fee shouldn't scale.
			let (gas_pool1, remaining_balance_pool1) =
				apply_extrinsic_and_calculate_gas_fee(lp, call.clone()).unwrap();
			let (gas_pool2, remaining_balance_pool2) =
				apply_extrinsic_and_calculate_gas_fee(lp, call2.clone()).unwrap();
			assert_eq!(gas_pool1, gas_pool2);
			assert!(remaining_balance_pool2 < remaining_balance_pool1);

			// Again, once each on each pool.
			let (gas_pool1_again, remaining_balance_pool1_again) =
				apply_extrinsic_and_calculate_gas_fee(lp, call.clone()).unwrap();
			assert!(remaining_balance_pool1_again < remaining_balance_pool2);
			let (gas_pool2_again, remaining_balance_pool2_again) =
				apply_extrinsic_and_calculate_gas_fee(lp, call2.clone()).unwrap();
			assert!(gas_pool1_again > gas_pool1);
			assert!(gas_pool2_again > gas_pool2);
			assert_eq!(gas_pool1_again, gas_pool2_again);
			assert!(remaining_balance_pool2_again < remaining_balance_pool2);
		});
}
