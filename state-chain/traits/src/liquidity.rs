use cf_chains::address::ForeignChainAddress;
use cf_primitives::{Asset, AssetAmount};
use frame_support::dispatch::DispatchError;
use sp_runtime::DispatchResult;

pub trait SwapIntentHandler {
	type AccountId;

	fn on_swap_ingress(
		ingress_address: ForeignChainAddress,
		from: Asset,
		to: Asset,
		amount: AssetAmount,
		egress_address: ForeignChainAddress,
		relayer_id: Self::AccountId,
		relayer_commission_bps: u16,
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
	// Attempt to swap `from` asset to `to` asset.
	// If OK, return (output_amount, input_asset_fee, stable_asset_fee)
	fn swap(
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError>;
}

impl SwappingApi for () {
	fn swap(
		_from: Asset,
		_to: Asset,
		_input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		Ok(Default::default())
	}
}

// TODO Remove these in favour of a real mocks.
impl<T: frame_system::Config> SwapIntentHandler for T {
	type AccountId = T::AccountId;

	fn on_swap_ingress(
		_ingress_address: ForeignChainAddress,
		_from: Asset,
		_to: Asset,
		_amount: AssetAmount,
		_egress_address: ForeignChainAddress,
		_relayer_id: Self::AccountId,
		_relayer_commission_bps: u16,
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
