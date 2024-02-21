use anyhow::{bail, Context, Result};
use async_trait::async_trait;
pub use cf_amm::{
	common::{Amount, PoolPairsMap, Side, Tick},
	range_orders::Liquidity,
};
use cf_chains::address::EncodedAddress;
use cf_primitives::{Asset, AssetAmount, BasisPoints, BlockNumber, EgressId};
use chainflip_engine::state_chain_observer::client::{
	extrinsic_api::signed::{SignedExtrinsicApi, UntilInBlock, WaitFor, WaitForResult},
	StateChainClient,
};
use pallet_cf_pools::{IncreaseOrDecrease, OrderId, RangeOrderSize};
use serde::{Deserialize, Serialize};
use sp_core::{H256, U256};
use state_chain_runtime::RuntimeCall;
use std::ops::Range;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ApiWaitForResult<T> {
	TxHash(H256),
	TxDetails { tx_hash: H256, response: T },
}

impl<T> ApiWaitForResult<T> {
	pub fn map_details<R>(self, f: impl FnOnce(T) -> R) -> ApiWaitForResult<R> {
		match self {
			ApiWaitForResult::TxHash(hash) => ApiWaitForResult::TxHash(hash),
			ApiWaitForResult::TxDetails { response, tx_hash } =>
				ApiWaitForResult::TxDetails { tx_hash, response: f(response) },
		}
	}

	#[track_caller]
	pub fn unwrap_details(self) -> T {
		match self {
			ApiWaitForResult::TxHash(_) => panic!("unwrap_details called on TransactionHash"),
			ApiWaitForResult::TxDetails { response, .. } => response,
		}
	}
}

pub mod types {
	use super::*;
	use cf_chains::assets::any::OldAsset;

	#[derive(Serialize, Deserialize, Clone)]
	pub struct RangeOrder {
		pub base_asset: OldAsset,
		pub quote_asset: OldAsset,
		pub id: U256,
		pub tick_range: Range<Tick>,
		pub liquidity_total: U256,
		pub collected_fees: PoolPairsMap<U256>,
		pub size_change: Option<IncreaseOrDecrease<RangeOrderChange>>,
	}

	#[derive(Serialize, Deserialize, Clone)]
	pub struct RangeOrderChange {
		pub liquidity: U256,
		pub amounts: PoolPairsMap<U256>,
	}

	#[derive(Serialize, Deserialize, Clone)]
	pub struct LimitOrder {
		pub base_asset: OldAsset,
		pub quote_asset: OldAsset,
		pub side: Side,
		pub id: U256,
		pub tick: Tick,
		pub sell_amount_total: U256,
		pub collected_fees: U256,
		pub bought_amount: U256,
		pub sell_amount_change: Option<IncreaseOrDecrease<U256>>,
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
					quote_asset,
					id,
					..
				},
			) => Some(types::RangeOrder {
				base_asset: base_asset.into(),
				quote_asset: quote_asset.into(),
				id: id.into(),
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
					sell_amount_change,
					sell_amount_total,
					collected_fees,
					bought_amount,
					tick,
					base_asset,
					quote_asset,
					side,
					id,
					..
				},
			) => Some(types::LimitOrder {
				base_asset: base_asset.into(),
				quote_asset: quote_asset.into(),
				side,
				id: id.into(),
				tick,
				sell_amount_total: sell_amount_total.into(),
				collected_fees: collected_fees.into(),
				bought_amount: bought_amount.into(),
				sell_amount_change: sell_amount_change
					.map(|increase_or_decrese| increase_or_decrese.map(|amount| amount.into())),
			}),
			_ => None,
		})
		.collect()
}

impl LpApi for StateChainClient {}

fn into_api_wait_for_result<T>(
	from: WaitForResult,
	map_events: impl FnOnce(Vec<state_chain_runtime::RuntimeEvent>) -> T,
) -> ApiWaitForResult<T> {
	match from {
		WaitForResult::TransactionHash(tx_hash) => ApiWaitForResult::TxHash(tx_hash),
		WaitForResult::Details(details) => {
			let (tx_hash, events, ..) = details;
			ApiWaitForResult::TxDetails { tx_hash, response: map_events(events) }
		},
	}
}

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

	async fn request_liquidity_deposit_address(
		&self,
		asset: Asset,
		wait_for: WaitFor,
		boost_fee: Option<BasisPoints>,
	) -> Result<ApiWaitForResult<EncodedAddress>> {
		let wait_for_result = self
			.submit_signed_extrinsic_wait_for(
				pallet_cf_lp::Call::request_liquidity_deposit_address {
					asset,
					boost_fee: boost_fee.unwrap_or_default(),
				},
				wait_for,
			)
			.await?;

		Ok(match wait_for_result {
			WaitForResult::TransactionHash(tx_hash) => return Ok(ApiWaitForResult::TxHash(tx_hash)),
			WaitForResult::Details(details) => {
				let (tx_hash, events, ..) = details;
				let encoded_address = events
					.into_iter()
					.find_map(|event| match event {
						state_chain_runtime::RuntimeEvent::LiquidityProvider(
							pallet_cf_lp::Event::LiquidityDepositAddressReady {
								deposit_address,
								..
							},
						) => Some(deposit_address),
						_ => None,
					})
					.ok_or_else(|| {
						anyhow::anyhow!("No LiquidityDepositAddressReady event was found")
					})?;

				ApiWaitForResult::TxDetails { tx_hash, response: encoded_address }
			},
		})
	}

	async fn withdraw_asset(
		&self,
		amount: AssetAmount,
		asset: Asset,
		destination_address: EncodedAddress,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<EgressId>> {
		if amount == 0 {
			bail!("Withdrawal amount must be greater than 0");
		}

		let wait_for_result = self
			.submit_signed_extrinsic_wait_for(
				pallet_cf_lp::Call::withdraw_asset { amount, asset, destination_address },
				wait_for,
			)
			.await?;

		Ok(match wait_for_result {
			WaitForResult::TransactionHash(tx_hash) => return Ok(ApiWaitForResult::TxHash(tx_hash)),
			WaitForResult::Details(details) => {
				let (tx_hash, events, ..) = details;
				let egress_id = events
					.into_iter()
					.find_map(|event| match event {
						state_chain_runtime::RuntimeEvent::LiquidityProvider(
							pallet_cf_lp::Event::WithdrawalEgressScheduled { egress_id, .. },
						) => Some(egress_id),
						_ => None,
					})
					.ok_or_else(|| {
						anyhow::anyhow!("No WithdrawalEgressScheduled event was found")
					})?;

				ApiWaitForResult::TxDetails { tx_hash, response: egress_id }
			},
		})
	}

	async fn update_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderId,
		option_tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSize>,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<Vec<types::RangeOrder>>> {
		// Submit the mint order
		Ok(into_api_wait_for_result(
			self.submit_signed_extrinsic_wait_for(
				pallet_cf_pools::Call::update_range_order {
					base_asset,
					quote_asset,
					id,
					option_tick_range,
					size_change,
				},
				wait_for,
			)
			.await?,
			collect_range_order_returns,
		))
	}

	async fn set_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderId,
		option_tick_range: Option<Range<Tick>>,
		size: RangeOrderSize,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<Vec<types::RangeOrder>>> {
		// Submit the mint order
		Ok(into_api_wait_for_result(
			self.submit_signed_extrinsic_wait_for(
				pallet_cf_pools::Call::set_range_order {
					base_asset,
					quote_asset,
					id,
					option_tick_range,
					size,
				},
				wait_for,
			)
			.await?,
			collect_range_order_returns,
		))
	}

	#[allow(clippy::too_many_arguments)]
	async fn update_limit_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		option_tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<AssetAmount>,
		dispatch_at: Option<BlockNumber>,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<Vec<types::LimitOrder>>> {
		self.scheduled_or_immediate(
			pallet_cf_pools::Call::update_limit_order {
				base_asset,
				quote_asset,
				side,
				id,
				option_tick,
				amount_change,
			},
			dispatch_at,
			wait_for,
		)
		.await
	}

	#[allow(clippy::too_many_arguments)]
	async fn set_limit_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		option_tick: Option<Tick>,
		sell_amount: AssetAmount,
		dispatch_at: Option<BlockNumber>,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<Vec<types::LimitOrder>>> {
		self.scheduled_or_immediate(
			pallet_cf_pools::Call::set_limit_order {
				base_asset,
				quote_asset,
				side,
				id,
				option_tick,
				sell_amount,
			},
			dispatch_at,
			wait_for,
		)
		.await
	}

	async fn scheduled_or_immediate(
		&self,
		call: pallet_cf_pools::Call<state_chain_runtime::Runtime>,
		dispatch_at: Option<BlockNumber>,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<Vec<types::LimitOrder>>> {
		Ok(into_api_wait_for_result(
			if let Some(dispatch_at) = dispatch_at {
				self.submit_signed_extrinsic_wait_for(
					pallet_cf_pools::Call::schedule_limit_order_update {
						call: Box::new(call),
						dispatch_at,
					},
					wait_for,
				)
				.await?
			} else {
				self.submit_signed_extrinsic_wait_for(call, wait_for).await?
			},
			collect_limit_order_returns,
		))
	}
}
