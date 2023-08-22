use std::ops::Range;

use anyhow::{Context, Result};
use async_trait::async_trait;
pub use cf_amm::{
	common::{Order, SideMap, Tick},
	range_orders::Liquidity,
};
use cf_chains::address::EncodedAddress;
use cf_primitives::{Asset, AssetAmount, EgressId};
use chainflip_engine::state_chain_observer::client::{
	extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
	StateChainClient,
};
use pallet_cf_pools::{AssetAmounts, IncreaseOrDecrease, OrderId, RangeOrderSize};
use serde::{Deserialize, Serialize};
use sp_core::H256;
use state_chain_runtime::RuntimeCall;

#[derive(Serialize, Deserialize)]
pub struct UpdateRangeOrderReturn {
	liquidity_delta: Liquidity,
	liquidity_total: Liquidity,
	assets_delta: AssetAmounts,
	collected_fees: AssetAmounts,
}

#[derive(Serialize, Deserialize)]
pub struct SetRangeOrderReturn {
	increase_or_decrease: IncreaseOrDecrease,
	liquidity_delta: Liquidity,
	liquidity_total: Liquidity,
	assets_delta: AssetAmounts,
	collected_fees: AssetAmounts,
}

#[derive(Serialize, Deserialize)]
pub struct UpdateLimitOrderReturn {
	amount_delta: AssetAmount,
	amount_total: AssetAmount,
	collected_fees: AssetAmount,
	swapped_liquidity: AssetAmount,
}

#[derive(Serialize, Deserialize)]
pub struct SetLimitOrderReturn {
	increase_or_decrease: IncreaseOrDecrease,
	amount_delta: AssetAmount,
	amount_total: AssetAmount,
	collected_fees: AssetAmount,
	swapped_liquidity: AssetAmount,
}

impl LpApi for StateChainClient {}

#[async_trait]
pub trait LpApi: SignedExtrinsicApi {
	async fn register_emergency_withdrawal_address(&self, address: EncodedAddress) -> Result<H256> {
		let (tx_hash, ..) = self
			.submit_signed_extrinsic(RuntimeCall::from(
				pallet_cf_lp::Call::register_emergency_withdrawal_address { address },
			))
			.await
			.until_finalized()
			.await
			.context("Registration for Emergency Withdrawal address failed.")?;
		Ok(tx_hash)
	}

	async fn request_liquidity_deposit_address(&self, asset: Asset) -> Result<EncodedAddress> {
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_lp::Call::request_liquidity_deposit_address {
				asset,
			})
			.await
			.until_finalized()
			.await?;

		Ok(events
			.into_iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::LiquidityProvider(
					pallet_cf_lp::Event::LiquidityDepositAddressReady { deposit_address, .. },
				) => Some(deposit_address),
				_ => None,
			})
			.expect("DepositAddressReady must have been generated"))
	}

	async fn withdraw_asset(
		&self,
		amount: AssetAmount,
		asset: Asset,
		destination_address: EncodedAddress,
	) -> Result<EgressId> {
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_lp::Call::withdraw_asset {
				amount,
				asset,
				destination_address,
			})
			.await
			.until_finalized()
			.await?;

		Ok(events
			.into_iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::LiquidityProvider(
					pallet_cf_lp::Event::WithdrawalEgressScheduled { egress_id, .. },
				) => Some(egress_id),
				_ => None,
			})
			.expect("WithdrawalEgressScheduled must have been generated"))
	}

	async fn update_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		id: OrderId,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSize,
		increase_or_decrease: IncreaseOrDecrease,
	) -> Result<UpdateRangeOrderReturn> {
		// Submit the mint order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::update_range_order {
				base_asset,
				pair_asset,
				id,
				tick_range,
				size,
				increase_or_decrease,
			})
			.await
			.until_finalized()
			.await?;

		// Get some details from the emitted event
		Ok({
			let (liquidity_delta, liquidity_total, assets_delta) = events
				.iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::RangeOrderUpdated {
							increase_or_decrease: event_mint_or_burn,
							liquidity_delta,
							liquidity_total,
							assets_delta,
							..
						},
					) if increase_or_decrease == *event_mint_or_burn =>
						Some((*liquidity_delta, *liquidity_total, *assets_delta)),
					_ => None,
				})
				.expect("RangeOrderUpdated must have been generated");

			let collected_fees = events
				.iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::RangeOrderCollectedEarnings {
							collected_fees, ..
						},
					) => Some(*collected_fees),
					_ => None,
				})
				.expect("RangeOrderCollectedEarnings must have been generated");

			UpdateRangeOrderReturn {
				liquidity_delta,
				liquidity_total,
				assets_delta,
				collected_fees,
			}
		})
	}

	async fn set_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		id: OrderId,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSize,
	) -> Result<SetRangeOrderReturn> {
		// Submit the mint order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::set_range_order {
				base_asset,
				pair_asset,
				id,
				tick_range,
				size,
			})
			.await
			.until_finalized()
			.await?;

		// Get some details from the emitted event
		Ok({
			let (increase_or_decrease, liquidity_delta, liquidity_total, assets_delta) = events
				.iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::RangeOrderUpdated {
							increase_or_decrease,
							liquidity_delta,
							liquidity_total,
							assets_delta,
							..
						},
					) => Some((
						*increase_or_decrease,
						*liquidity_delta,
						*liquidity_total,
						*assets_delta,
					)),
					_ => None,
				})
				.expect("RangeOrderUpdated must have been generated");

			let collected_fees = events
				.iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::RangeOrderCollectedEarnings {
							collected_fees, ..
						},
					) => Some(*collected_fees),
					_ => None,
				})
				.expect("RangeOrderCollectedEarnings must have been generated");

			SetRangeOrderReturn {
				increase_or_decrease,
				liquidity_delta,
				liquidity_total,
				assets_delta,
				collected_fees,
			}
		})
	}

	async fn update_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		tick: Option<Tick>,
		sell_amount: AssetAmount,
		increase_or_decrease: IncreaseOrDecrease,
	) -> Result<UpdateLimitOrderReturn> {
		// Submit the mint order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::update_limit_order {
				sell_asset,
				buy_asset,
				id,
				tick,
				sell_amount,
				increase_or_decrease,
			})
			.await
			.until_finalized()
			.await?;

		// Get some details from the emitted event
		Ok({
			let (amount_delta, amount_total) = events
				.iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::LimitOrderUpdated {
							increase_or_decrease: event_mint_or_burn,
							amount_delta,
							amount_total,
							..
						},
					) if increase_or_decrease == *event_mint_or_burn => Some((*amount_delta, *amount_total)),
					_ => None,
				})
				.expect("LimitOrderUpdated must have been generated");

			let (collected_fees, swapped_liquidity) = events
				.iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::LimitOrderCollectedEarnings {
							collected_fees,
							swapped_liquidity,
							..
						},
					) => Some((*collected_fees, *swapped_liquidity)),
					_ => None,
				})
				.expect("LimitOrderCollectedEarnings must have been generated");

			UpdateLimitOrderReturn { amount_delta, amount_total, collected_fees, swapped_liquidity }
		})
	}

	async fn set_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		tick: Option<Tick>,
		sell_amount: AssetAmount,
	) -> Result<SetLimitOrderReturn> {
		// Submit the burn order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::set_limit_order {
				sell_asset,
				buy_asset,
				id,
				tick,
				sell_amount,
			})
			.await
			.until_finalized()
			.await?;

		// Get some details from the emitted event
		Ok({
			let (increase_or_decrease, amount_delta, amount_total) = events
				.iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::LimitOrderUpdated {
							increase_or_decrease,
							amount_delta,
							amount_total,
							..
						},
					) => Some((*increase_or_decrease, *amount_delta, *amount_total)),
					_ => None,
				})
				.expect("LimitOrderUpdated must have been generated");

			let (collected_fees, swapped_liquidity) = events
				.iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::LimitOrderCollectedEarnings {
							collected_fees,
							swapped_liquidity,
							..
						},
					) => Some((*collected_fees, *swapped_liquidity)),
					_ => None,
				})
				.expect("LimitOrderCollectedEarnings must have been generated");

			SetLimitOrderReturn {
				increase_or_decrease,
				amount_delta,
				amount_total,
				collected_fees,
				swapped_liquidity,
			}
		})
	}
}
