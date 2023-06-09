use cf_chains::address::ForeignChainAddress;
use cf_primitives::{Asset, AssetAmount, BasisPoints, ChannelId, SwapLeg};
use frame_support::dispatch::DispatchError;
use sp_runtime::DispatchResult;

pub trait SwapDepositHandler {
	type AccountId;

	#[allow(clippy::too_many_arguments)]
	fn schedule_swap_from_channel(
		deposit_address: ForeignChainAddress,
		from: Asset,
		to: Asset,
		amount: AssetAmount,
		destination_address: ForeignChainAddress,
		broker_id: Self::AccountId,
		broker_commission_bps: BasisPoints,
		channel_id: ChannelId,
	);
}

pub trait LpBalanceApi {
	type AccountId;

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
}

pub trait SwappingApi {
	/// Takes the swap amount in STABLE_ASSET, collect network fee from it
	/// and return the remaining value
	fn take_network_fee(input_amount: AssetAmount) -> AssetAmount;

	/// Process a single leg of a swap, into or from Stable asset. No network fee is taken.
	fn swap_single_leg(
		leg: SwapLeg,
		unstable_asset: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError>;
}

impl<T: frame_system::Config> SwappingApi for T {
	fn take_network_fee(input_amount: AssetAmount) -> AssetAmount {
		input_amount
	}

	fn swap_single_leg(
		_leg: SwapLeg,
		_unstable_asset: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		Ok(input_amount)
	}
}

// TODO Remove these in favour of a real mocks.
impl<T: frame_system::Config> SwapDepositHandler for T {
	type AccountId = T::AccountId;

	fn schedule_swap_from_channel(
		_deposit_address: ForeignChainAddress,
		_from: Asset,
		_to: Asset,
		_amount: AssetAmount,
		_destination_address: ForeignChainAddress,
		_broker_id: Self::AccountId,
		_broker_commission_bps: BasisPoints,
		_channel_id: ChannelId,
	) {
	}
}

impl<T: frame_system::Config> LpBalanceApi for T {
	type AccountId = T::AccountId;

	fn try_credit_account(
		_who: &Self::AccountId,
		_asset: Asset,
		_amount: AssetAmount,
	) -> DispatchResult {
		// TODO
		Ok(())
	}

	fn try_debit_account(
		_who: &Self::AccountId,
		_asset: Asset,
		_amount: AssetAmount,
	) -> DispatchResult {
		// TODO
		Ok(())
	}
}
