use cf_primitives::{AccountRole, Asset, AuthorityCount, FLIPPERINOS_PER_FLIP};
use cf_traits::IncreaseOrDecrease;
use codec::Encode;
use frame_support::pallet_prelude::TransactionValidityError;
use pallet_cf_flip::{FeeScalingRate, FeeScalingRateConfig};
use pallet_cf_pools::RangeOrderSize;
use sp_block_builder::runtime_decl_for_block_builder::BlockBuilderV6;
use sp_keyring::test::AccountKeyring;
use sp_runtime::{generic::Era, MultiSignature};
use state_chain_runtime::{Balance, Flip, Runtime, RuntimeCall, SignedPayload, System};

pub fn apply_extrinsic_and_calculate_gas_fee(
	caller: AccountKeyring,
	call: RuntimeCall,
) -> Result<(Balance, Balance), TransactionValidityError> {
	let caller_account_id = caller.to_account_id();
	let before = Flip::total_balance_of(&caller_account_id);

	let extra = (
		frame_system::CheckNonZeroSender::<Runtime>::new(),
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckTxVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckEra::<Runtime>::from(Era::Immortal),
		frame_system::CheckNonce::<Runtime>::from(System::account_nonce(&caller_account_id)),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0u128),
		frame_metadata_hash_extension::CheckMetadataHash::<Runtime>::new(false),
	);

	let signed_payload = SignedPayload::new(call.clone(), extra.clone()).unwrap();
	let signature = MultiSignature::Ed25519(caller.sign(&signed_payload.encode()));
	let ext = sp_runtime::generic::UncheckedExtrinsic::new_signed(
		call,
		caller_account_id.clone().into(),
		signature,
		extra,
	);

	let _ = Runtime::apply_extrinsic(ext)?;

	let after = Flip::total_balance_of(&caller_account_id);

	Ok((before - after, after))
}

const fn update_range_order_call(base_asset: Asset) -> RuntimeCall {
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

const UPDATE_ETH_RANGE_ORDER: RuntimeCall = update_range_order_call(Asset::Eth);
const UPDATE_BTC_RANGE_ORDER: RuntimeCall = update_range_order_call(Asset::Btc);

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

			assert_eq!(FeeScalingRate::<Runtime>::get(), FeeScalingRateConfig::NoScaling);

			let fees = (1u16..=10)
				.map(|call_count| {
					(
						call_count,
						apply_extrinsic_and_calculate_gas_fee(lp, UPDATE_ETH_RANGE_ORDER).unwrap(),
					)
				})
				.collect::<Vec<_>>();

			for ((_, (gas, remaining_balance)), (_, (next_gas, next_remaining_balance))) in
				fees.iter().zip(fees.iter().skip(1))
			{
				assert_eq!(next_gas, gas);
				assert_eq!(*next_remaining_balance, remaining_balance - next_gas);
			}

			// Reset the fee scaling counters
			testnet.move_forward_blocks(1);

			// Set the config to scale linearly per pool.
			const THRESHOLD: u16 = 2;
			FeeScalingRate::<Runtime>::set(FeeScalingRateConfig::DelayedExponential {
				threshold: THRESHOLD,
				exponent: 1,
			});

			let fees = (1u16..=10)
				.map(|call_count| {
					(
						call_count,
						apply_extrinsic_and_calculate_gas_fee(lp, UPDATE_ETH_RANGE_ORDER).unwrap(),
					)
				})
				.collect::<Vec<_>>();

			let mut fee_increase = None;
			for (
				(call_count, (gas, remaining_balance)),
				(next_call_count, (next_gas, next_remaining_balance)),
			) in fees.iter().zip(fees.iter().skip(1))
			{
				if *next_call_count > THRESHOLD {
					assert!(next_gas > gas, "Call {call_count} vs {next_call_count} in {:?}", fees);
					let last_fee_increase = fee_increase.replace(next_gas - gas);
					if let Some(last_fee_increase) = last_fee_increase {
						assert_eq!(
							next_gas - gas,
							last_fee_increase,
							"Expected linear increase at call count {next_call_count} in {:?}",
							fees
						);
					}
				} else {
					assert_eq!(
						next_gas, gas,
						"Call {call_count} vs {next_call_count} in {:?}",
						fees
					);
				}
				assert_eq!(*next_remaining_balance, remaining_balance - next_gas);
			}

			// Using a different pool should scale independently, starting from the same value.
			let fees_btc = (1u16..=10)
				.map(|call_count| {
					(
						call_count,
						apply_extrinsic_and_calculate_gas_fee(lp, UPDATE_BTC_RANGE_ORDER).unwrap(),
					)
				})
				.collect::<Vec<_>>();

			assert_eq!(
				fees_btc.iter().map(|(_, (gas, _))| gas).collect::<Vec<_>>(),
				fees.iter().map(|(_, (gas, _))| gas).collect::<Vec<_>>(),
				"Expected same fees for BTC and ETH",
			);
		});
}
