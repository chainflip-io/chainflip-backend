use anyhow::{Context, Result};
use async_trait::async_trait;
pub use cf_amm::{
	common::{SideMap, Tick},
	range_orders::Liquidity,
};
use cf_chains::address::EncodedAddress;
use cf_primitives::{Asset, AssetAmount, EgressId};
use chainflip_engine::state_chain_observer::client::{
	extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
	StateChainClient,
};
pub use core::ops::Range;
pub use pallet_cf_pools::{utilities as pool_utilities, Order as BuyOrSellOrder, RangeOrderSize};
use serde::{Deserialize, Serialize};
use sp_core::H256;
use state_chain_runtime::RuntimeCall;

#[derive(Serialize, Deserialize, Debug)]
pub struct MintRangeOrderReturn {
	assets_debited: SideMap<AssetAmount>,
	collected_fees: SideMap<AssetAmount>,
}

#[derive(Serialize, Deserialize)]
pub struct BurnRangeOrderReturn {
	assets_credited: SideMap<AssetAmount>,
	collected_fees: SideMap<AssetAmount>,
}

#[derive(Serialize, Deserialize)]
pub struct MintLimitOrderReturn {
	assets_debited: AssetAmount,
	collected_fees: AssetAmount,
	swapped_liquidity: AssetAmount,
}

#[derive(Serialize, Deserialize)]
pub struct BurnLimitOrderReturn {
	assets_credited: AssetAmount,
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

	async fn mint_range_order(
		&self,
		asset: Asset,
		range: Range<Tick>,
		order_size: RangeOrderSize,
	) -> Result<MintRangeOrderReturn> {
		// Submit the mint order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_mint_range_order {
				unstable_asset: asset,
				price_range_in_ticks: range,
				order_size,
			})
			.await
			.until_finalized()
			.await?;

		// Get some details from the emitted event
		Ok(events
			.into_iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::LiquidityPools(
					pallet_cf_pools::Event::RangeOrderMinted {
						assets_debited, collected_fees, ..
					},
				) => Some(MintRangeOrderReturn { assets_debited, collected_fees }),
				_ => None,
			})
			.expect("RangeOrderMinted must have been generated"))
	}

	async fn burn_range_order(
		&self,
		asset: Asset,
		range: Range<Tick>,
		amount: AssetAmount,
	) -> Result<BurnRangeOrderReturn> {
		// TODO: Re-enable this check after #3082 in implemented
		// Find the current position and calculate new target amount
		// if get_liquidity_at_position(&state_chain_client, asset, range,
		// latest_block_hash) 	.await? < amount
		// {
		// 	bail!("Insufficient minted liquidity at position");
		// }

		// Submit the burn call
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_burn_range_order {
				unstable_asset: asset,
				price_range_in_ticks: range,
				liquidity: amount,
			})
			.await
			.until_finalized()
			.await?;

		// Get some details from the emitted event
		Ok(events
			.into_iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::LiquidityPools(
					pallet_cf_pools::Event::RangeOrderBurned {
						assets_credited,
						collected_fees,
						..
					},
				) => Some(BurnRangeOrderReturn { assets_credited, collected_fees }),
				_ => None,
			})
			.expect("RangeOrderBurned must have been generated"))
	}

	async fn mint_limit_order(
		&self,
		asset: Asset,
		order: BuyOrSellOrder,
		price: Tick,
		amount: AssetAmount,
	) -> Result<MintLimitOrderReturn> {
		// Submit the mint order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_mint_limit_order {
				unstable_asset: asset,
				order,
				price_as_tick: price,
				amount,
			})
			.await
			.until_finalized()
			.await?;

		// Get some details from the emitted event
		Ok(events
			.into_iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::LiquidityPools(
					pallet_cf_pools::Event::LimitOrderMinted {
						assets_debited,
						collected_fees,
						swapped_liquidity,
						..
					},
				) =>
					Some(MintLimitOrderReturn { assets_debited, collected_fees, swapped_liquidity }),
				_ => None,
			})
			.expect("LimitOrderMinted must have been generated"))
	}

	async fn burn_limit_order(
		&self,
		asset: Asset,
		order: BuyOrSellOrder,
		price: Tick,
		amount: AssetAmount,
	) -> Result<BurnLimitOrderReturn> {
		// TODO: get limit orders and see if there's enough liquidity to burn

		// Submit the burn order
		let (_tx_hash, events, ..) = self
			.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_burn_limit_order {
				unstable_asset: asset,
				order,
				price_as_tick: price,
				amount,
			})
			.await
			.until_finalized()
			.await?;

		// Get some details from the emitted event
		Ok(events
			.into_iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::LiquidityPools(
					pallet_cf_pools::Event::LimitOrderBurned {
						assets_credited,
						collected_fees,
						swapped_liquidity,
						..
					},
				) => Some(BurnLimitOrderReturn {
					assets_credited,
					collected_fees,
					swapped_liquidity,
				}),
				_ => None,
			})
			.expect("LimitOrderBurned must have been generated"))
	}
}
