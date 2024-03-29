use cf_chains::{address::ForeignChainAddress, assets::any::AssetMap};
use cf_primitives::{Asset, AssetAmount, BasisPoints, ChannelId, SwapId};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::{DispatchError, DispatchResult};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;

pub trait SwapDepositHandler {
	type AccountId;

	#[allow(clippy::too_many_arguments)]
	fn schedule_swap_from_channel(
		deposit_address: ForeignChainAddress,
		deposit_block_height: u64,
		from: Asset,
		to: Asset,
		amount: AssetAmount,
		destination_address: ForeignChainAddress,
		broker_id: Self::AccountId,
		broker_commission_bps: BasisPoints,
		channel_id: ChannelId,
	) -> SwapId;
}

pub trait LpDepositHandler {
	type AccountId;

	/// Attempt to credit the account with the given asset and amount
	/// as a result of a liquidity deposit.
	fn add_deposit(who: &Self::AccountId, asset: Asset, amount: AssetAmount) -> DispatchResult;
}

pub trait LpBalanceApi {
	type AccountId;

	#[cfg(feature = "runtime-benchmarks")]
	fn register_liquidity_refund_address(who: &Self::AccountId, address: ForeignChainAddress);

	fn ensure_has_refund_address_for_pair(
		who: &Self::AccountId,
		base_asset: Asset,
		quote_asset: Asset,
	) -> DispatchResult;

	/// Attempt to credit the account with the given asset and amount.
	fn try_credit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult;

	/// Attempt to debit the account with the given asset and amount.
	fn try_debit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult;

	/// Record the fees collected by the account.
	fn record_fees(who: &Self::AccountId, amount: AssetAmount, asset: Asset);

	/// Returns the asset balances of the given account.
	fn asset_balances(who: &Self::AccountId) -> Result<AssetMap<AssetAmount>, DispatchError>;
}

pub trait PoolApi {
	type AccountId;

	/// Sweep all earnings of an LP into their free balance (Should be called before any assets are
	/// debited from their free balance)
	fn sweep(who: &Self::AccountId) -> Result<(), DispatchError>;

	/// Returns the number of open orders for the given account and pair.
	fn open_order_count(
		who: &Self::AccountId,
		base_asset: Asset,
		quote_asset: Asset,
	) -> Result<u32, DispatchError>;
}

impl<T: frame_system::Config> PoolApi for T {
	type AccountId = T::AccountId;

	fn sweep(_who: &Self::AccountId) -> Result<(), DispatchError> {
		Ok(())
	}

	fn open_order_count(
		_who: &Self::AccountId,
		_base_asset: Asset,
		_quote_asset: Asset,
	) -> Result<u32, DispatchError> {
		Ok(0)
	}
}

pub trait SwappingApi {
	/// Takes the swap amount in STABLE_ASSET, collect network fee from it
	/// and return the remaining value
	fn take_network_fee(input_amount: AssetAmount) -> AssetAmount;

	/// Process a single leg of a swap, into or from Stable asset. No network fee is taken.
	fn swap_single_leg(
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError>;
}

impl<T: frame_system::Config> SwappingApi for T {
	fn take_network_fee(input_amount: AssetAmount) -> AssetAmount {
		input_amount
	}

	fn swap_single_leg(
		_from: Asset,
		_to: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		Ok(input_amount)
	}
}

pub trait SwapQueueApi {
	type BlockNumber;

	/// Add a swap to the internal swapping queue with the default block delay. Return swap_id along
	/// with the block at which the swap is scheduled to be executed.
	fn schedule_swap(
		from: Asset,
		to: Asset,
		amount: AssetAmount,
		swap_type: SwapType,
	) -> (u64, Self::BlockNumber);
}

impl<T: frame_system::Config> SwapQueueApi for T {
	type BlockNumber = BlockNumberFor<T>;

	fn schedule_swap(
		_from: Asset,
		_to: Asset,
		_amount: AssetAmount,
		_swap_type: SwapType,
	) -> (u64, Self::BlockNumber) {
		(0, Self::BlockNumber::default())
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SwapType {
	Swap(ForeignChainAddress),
	CcmPrincipal(SwapId),
	CcmGas(SwapId),
	NetworkFee,
}
