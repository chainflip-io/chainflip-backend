//! Chainflip transaction fees.
//!
//! The Chainflip network is permissioned and as such the main reasons for fees are (a) to encourage
//! 'good' behaviour and (b) to ensure that only funded actors can submit extrinsics to the network.

use crate::{imbalances::Surplus, Config as FlipConfig, Pallet as Flip};
use cf_primitives::ScalableFeeCallInfo;
use cf_traits::{TransactionFeeScaler, WaivedFees};
use frame_support::{
	pallet_prelude::InvalidTransaction,
	sp_runtime::{
		traits::{DispatchInfoOf, Get, Zero},
		FixedPointNumber, RuntimeDebug,
	},
	traits::Imbalance,
	DefaultNoBound,
};
use frame_system::Config;
use pallet_transaction_payment::{Config as TxConfig, OnChargeTransaction};
use sp_std::marker::PhantomData;

/// Marker struct for implementation of [OnChargeTransaction].
///
/// Fees are burned.
/// Tips are ignored.
/// Any excess fees are refunded to the caller.
pub struct FlipTransactionPayment<T>(PhantomData<T>);

#[derive(DefaultNoBound, RuntimeDebug)]
pub struct TransactionPaymentLiquidityInfo<T: crate::Config + FlipConfig> {
	pub imbalance: Option<Surplus<T>>,
	pub scalable_fee_call_info: ScalableFeeCallInfo<T::AccountId>,
}

impl<T: TxConfig + FlipConfig + Config> OnChargeTransaction<T> for FlipTransactionPayment<T> {
	type Balance = <T as FlipConfig>::Balance;
	type LiquidityInfo = TransactionPaymentLiquidityInfo<T>;

	fn withdraw_fee(
		who: &T::AccountId,
		call: &<T as frame_system::Config>::RuntimeCall,
		_dispatch_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		mut fee: Self::Balance,
		_tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, frame_support::unsigned::TransactionValidityError> {
		if T::WaivedFees::should_waive_fees(call, who) {
			return Ok(Default::default())
		}

		let scalable_fee_call_info = T::TransactionFeeScaler::call_info(call, who);

		if scalable_fee_call_info.scalable_fee() {
			fee = sp_std::cmp::max(fee, T::SpamPreventionUpfrontFee::get());
		};

		if let Some(surplus) = Flip::<T>::try_debit(who, fee) {
			if surplus.peek().is_zero() {
				Ok(Default::default())
			} else {
				Ok(TransactionPaymentLiquidityInfo {
					imbalance: Some(surplus),
					scalable_fee_call_info,
				})
			}
		} else {
			Err(InvalidTransaction::Payment.into())
		}
	}

	fn correct_and_deposit_fee(
		who: &T::AccountId,
		_dispatch_info: &sp_runtime::traits::DispatchInfoOf<
			<T as frame_system::Config>::RuntimeCall,
		>,
		_post_info: &sp_runtime::traits::PostDispatchInfoOf<
			<T as frame_system::Config>::RuntimeCall,
		>,
		corrected_fee: Self::Balance,
		_tip: Self::Balance,
		escrow: Self::LiquidityInfo,
	) -> Result<(), frame_support::unsigned::TransactionValidityError> {
		if let Some(surplus) = escrow.imbalance {
			// It's possible the account was deleted during extrinsic execution. If this is the
			// case, we shouldn't refund anything, we can just burn all fees in escrow.
			let to_burn = if frame_system::Pallet::<T>::account_exists(who) {
				corrected_fee
			} else {
				surplus.peek()
			};

			let to_burn =
				T::TransactionFeeScaler::get_fee_multiplier(escrow.scalable_fee_call_info)
					.saturating_mul_int(to_burn);

			// If there is a difference this will be reconciled when the result goes out of scope.
			let _imbalance = surplus.offset(Flip::<T>::burn(to_burn));
		}
		Ok(())
	}
}
