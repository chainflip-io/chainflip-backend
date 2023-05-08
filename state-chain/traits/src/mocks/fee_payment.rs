use sp_runtime::DispatchError;

use crate::{Chainflip, FeePayment};

use super::funding_info::MockFundingInfo;

pub struct MockFeePayment<T>(sp_std::marker::PhantomData<T>);

pub const ERROR_INSUFFICIENT_LIQUIDITY: DispatchError =
	DispatchError::Other("Insufficient liquidity");

impl<T: Chainflip<FundingInfo = MockFundingInfo<T>>> FeePayment for MockFeePayment<T> {
	type AccountId = T::AccountId;
	type Amount = T::Amount;

	fn try_burn_fee(
		account_id: &Self::AccountId,
		amount: Self::Amount,
	) -> sp_runtime::DispatchResult {
		MockFundingInfo::<T>::try_debit_funds(account_id, amount)
			.map(|_| ())
			.ok_or(ERROR_INSUFFICIENT_LIQUIDITY)
	}
}
