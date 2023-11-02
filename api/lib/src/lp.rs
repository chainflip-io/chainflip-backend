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
	extrinsic_api::signed::{SignedExtrinsicApi, UntilInBlock},
	StateChainClient,
};
use pallet_cf_pools::{AssetAmounts, IncreaseOrDecrease, OrderId, RangeOrderSize};
use serde::{Deserialize, Serialize};
use sp_core::H256;
use state_chain_runtime::RuntimeCall;

#[derive(Serialize, Deserialize, Clone)]
pub struct RangeOrderReturn {
	tick_range: Range<Tick>,
	liquidity_total: Liquidity,
	collected_fees: AssetAmounts,
	increase_or_decrease: IncreaseOrDecrease,
	liquidity_delta: Liquidity,
	assets_delta: AssetAmounts,
}

fn collect_range_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
	order_id: OrderId,
) -> Vec<RangeOrderReturn> {
	events
		.into_iter()
		.filter_map(|event| match event {
			state_chain_runtime::RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::RangeOrderUpdated {
					position_delta: Some((increase_or_decrease, liquidity_delta)),
					liquidity_total,
					assets_delta,
					collected_fees,
					tick_range,
					id,
					..
				},
			) if order_id == id => Some(RangeOrderReturn {
				liquidity_delta,
				liquidity_total,
				increase_or_decrease,
				tick_range,
				collected_fees,
				assets_delta,
			}),
			_ => None,
		})
		.collect()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LimitOrderReturn {
	tick: Tick,
	amount_total: AssetAmount,
	collected_fees: AssetAmount,
	bought_amount: AssetAmount,
	position_delta: Option<(IncreaseOrDecrease, AssetAmount)>,
}

fn collect_limit_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
	order_id: OrderId,
) -> Vec<LimitOrderReturn> {
	events
		.into_iter()
		.filter_map(|event| match event {
			state_chain_runtime::RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::LimitOrderUpdated {
					position_delta,
					amount_total,
					collected_fees,
					bought_amount,
					tick,
					id,
					..
				},
			) if order_id == id => Some(LimitOrderReturn {
				tick,
				amount_total,
				collected_fees,
				bought_amount,
				position_delta,
			}),
			_ => None,
		})
		.collect()
}

impl LpApi for StateChainClient {}

#[async_trait]
pub trait LpApi: SignedExtrinsicApi {
	async fn register_liquidity_refund_address(&self, address: EncodedAddress) -> Result<H256> {
		let (tx_hash, ..) = self
			.submit_signed_extrinsic(RuntimeCall::from(
				pallet_cf_lp::Call::register_liquidity_refund_address { address },
			))
			.await
			.until_in_block()
			.await
			.context("Registration for Liquidity Refund Address failed.")?;
		Ok(tx_hash)
	}

	async fn request_liquidity_deposit_address(&self, asset: Asset) -> Result<EncodedAddress> {
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_lp::Call::request_liquidity_deposit_address {
				asset,
			})
			.await
			.until_in_block()
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
			.until_in_block()
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
		option_tick_range: Option<Range<Tick>>,
		increase_or_decrease: IncreaseOrDecrease,
		size: RangeOrderSize,
	) -> Result<Vec<RangeOrderReturn>> {
		// Submit the mint order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::update_range_order {
				base_asset,
				pair_asset,
				id,
				option_tick_range,
				increase_or_decrease,
				size,
			})
			.await
			.until_in_block()
			.await?;

		Ok(collect_range_order_returns(events, id))
	}

	async fn set_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		id: OrderId,
		option_tick_range: Option<Range<Tick>>,
		size: RangeOrderSize,
	) -> Result<Vec<RangeOrderReturn>> {
		// Submit the mint order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::set_range_order {
				base_asset,
				pair_asset,
				id,
				option_tick_range,
				size,
			})
			.await
			.until_in_block()
			.await?;

		Ok(collect_range_order_returns(events, id))
	}

	async fn update_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		option_tick: Option<Tick>,
		increase_or_decrease: IncreaseOrDecrease,
		amount: AssetAmount,
	) -> Result<Vec<LimitOrderReturn>> {
		// Submit the mint order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::update_limit_order {
				sell_asset,
				buy_asset,
				id,
				option_tick,
				increase_or_decrease,
				amount,
			})
			.await
			.until_in_block()
			.await?;

		Ok(collect_limit_order_returns(events, id))
	}

	async fn set_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		option_tick: Option<Tick>,
		sell_amount: AssetAmount,
	) -> Result<Vec<LimitOrderReturn>> {
		// Submit the burn order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::set_limit_order {
				sell_asset,
				buy_asset,
				id,
				option_tick,
				sell_amount,
			})
			.await
			.until_in_block()
			.await?;

		Ok(collect_limit_order_returns(events, id))
	}
}
