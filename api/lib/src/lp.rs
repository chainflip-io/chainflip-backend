use anyhow::{bail, Context, Result};
use async_trait::async_trait;
pub use cf_amm::{
	common::{Order, SideMap, Tick},
	range_orders::Liquidity,
};
use cf_chains::address::EncodedAddress;
use cf_primitives::{Asset, AssetAmount, BlockNumber, EgressId};
use chainflip_engine::state_chain_observer::client::{
	extrinsic_api::signed::{SignedExtrinsicApi, UntilInBlock},
	StateChainClient,
};
use pallet_cf_pools::{AssetsMap, IncreaseOrDecrease, OrderId, RangeOrderSize};
use serde::{Deserialize, Serialize};
use sp_core::H256;
use state_chain_runtime::RuntimeCall;
use std::ops::Range;
use utilities::rpc::NumberOrHex;

pub mod types {
	use super::*;
	#[derive(Serialize, Deserialize, Clone)]
	pub struct RangeOrder {
		pub base_asset: Asset,
		pub pair_asset: Asset,
		pub id: OrderId,
		pub tick_range: Range<Tick>,
		pub liquidity_total: NumberOrHex,
		pub collected_fees: AssetsMap<NumberOrHex>,
		pub size_change: Option<IncreaseOrDecrease<RangeOrderChange>>,
	}

	#[derive(Serialize, Deserialize, Clone)]
	pub struct RangeOrderChange {
		pub liquidity: NumberOrHex,
		pub amounts: AssetsMap<NumberOrHex>,
	}

	#[derive(Serialize, Deserialize, Clone)]
	pub struct LimitOrder {
		pub sell_asset: Asset,
		pub buy_asset: Asset,
		pub id: OrderId,
		pub tick: Tick,
		pub amount_total: NumberOrHex,
		pub collected_fees: NumberOrHex,
		pub bought_amount: NumberOrHex,
		pub amount_change: Option<IncreaseOrDecrease<NumberOrHex>>,
	}
}

fn collect_range_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
) -> Vec<types::RangeOrder> {
	events
		.into_iter()
		.filter_map(|event| match event {
			state_chain_runtime::RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::RangeOrderUpdated {
					size_change,
					liquidity_total,
					collected_fees,
					tick_range,
					base_asset,
					pair_asset,
					id,
					..
				},
			) => Some(types::RangeOrder {
				base_asset,
				pair_asset,
				id,
				size_change: size_change.map(|increase_or_decrese| {
					increase_or_decrese.map(|range_order_change| types::RangeOrderChange {
						liquidity: range_order_change.liquidity.into(),
						amounts: range_order_change.amounts.map(|amount| amount.into()),
					})
				}),
				liquidity_total: liquidity_total.into(),
				tick_range,
				collected_fees: collected_fees.map(Into::into),
			}),
			_ => None,
		})
		.collect()
}

fn collect_limit_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
) -> Vec<types::LimitOrder> {
	events
		.into_iter()
		.filter_map(|event| match event {
			state_chain_runtime::RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::LimitOrderUpdated {
					amount_change,
					amount_total,
					collected_fees,
					bought_amount,
					tick,
					sell_asset,
					buy_asset,
					id,
					..
				},
			) => Some(types::LimitOrder {
				sell_asset,
				buy_asset,
				id,
				tick,
				amount_total: amount_total.into(),
				collected_fees: collected_fees.into(),
				bought_amount: bought_amount.into(),
				amount_change: amount_change
					.map(|increase_or_decrese| increase_or_decrese.map(|amount| amount.into())),
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

		events
			.into_iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::LiquidityProvider(
					pallet_cf_lp::Event::LiquidityDepositAddressReady { deposit_address, .. },
				) => Some(deposit_address),
				_ => None,
			})
			.ok_or_else(|| anyhow::anyhow!("No LiquidityDepositAddressReady event was found"))
	}

	async fn withdraw_asset(
		&self,
		amount: AssetAmount,
		asset: Asset,
		destination_address: EncodedAddress,
	) -> Result<EgressId> {
		if amount == 0 {
			bail!("Withdrawal amount must be greater than 0");
		}

		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_lp::Call::withdraw_asset {
				amount,
				asset,
				destination_address,
			})
			.await
			.until_in_block()
			.await?;

		events
			.into_iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::LiquidityProvider(
					pallet_cf_lp::Event::WithdrawalEgressScheduled { egress_id, .. },
				) => Some(egress_id),
				_ => None,
			})
			.ok_or_else(|| anyhow::anyhow!("No WithdrawalEgressScheduled event was found"))
	}

	async fn update_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		id: OrderId,
		option_tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSize>,
	) -> Result<Vec<types::RangeOrder>> {
		// Submit the mint order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::update_range_order {
				base_asset,
				pair_asset,
				id,
				option_tick_range,
				size_change,
			})
			.await
			.until_in_block()
			.await?;

		Ok(collect_range_order_returns(events))
	}

	async fn set_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		id: OrderId,
		option_tick_range: Option<Range<Tick>>,
		size: RangeOrderSize,
	) -> Result<Vec<types::RangeOrder>> {
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

		Ok(collect_range_order_returns(events))
	}

	async fn update_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		option_tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<AssetAmount>,
		dispatch_at: Option<BlockNumber>,
	) -> Result<Vec<types::LimitOrder>> {
		self.scheduled_or_immediate(
			pallet_cf_pools::Call::update_limit_order {
				sell_asset,
				buy_asset,
				id,
				option_tick,
				amount_change,
			},
			dispatch_at,
		)
		.await
	}

	async fn set_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		option_tick: Option<Tick>,
		sell_amount: AssetAmount,
		dispatch_at: Option<BlockNumber>,
	) -> Result<Vec<types::LimitOrder>> {
		self.scheduled_or_immediate(
			pallet_cf_pools::Call::set_limit_order {
				sell_asset,
				buy_asset,
				id,
				option_tick,
				sell_amount,
			},
			dispatch_at,
		)
		.await
	}

	async fn scheduled_or_immediate(
		&self,
		call: pallet_cf_pools::Call<state_chain_runtime::Runtime>,
		dispatch_at: Option<BlockNumber>,
	) -> Result<Vec<types::LimitOrder>> {
		let events = if let Some(dispatch_at) = dispatch_at {
			let (_tx_hash, events, ..) = self
				.submit_signed_extrinsic(pallet_cf_pools::Call::schedule_limit_order_update {
					call: Box::new(call),
					dispatch_at,
				})
				.await
				.until_in_block()
				.await?;
			events
		} else {
			let (_tx_hash, events, ..) =
				self.submit_signed_extrinsic(call).await.until_in_block().await?;
			events
		};
		Ok(collect_limit_order_returns(events))
	}
}
