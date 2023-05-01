use sp_runtime::DispatchError;

use crate::{Chainflip, FeePayment};

use super::staking_info::MockStakingInfo;

pub struct MockFeePayment<T>(sp_std::marker::PhantomData<T>);

pub const ERROR_INSUFFICIENT_LIQUIDITY: DispatchError =
	DispatchError::Other("Insufficient liquidity");

impl<T: Chainflip<StakingInfo = MockStakingInfo<T>>> FeePayment for MockFeePayment<T> {
	type AccountId = T::AccountId;
	type Amount = T::Amount;

	fn try_burn_fee(
		account_id: &Self::AccountId,
		amount: Self::Amount,
	) -> sp_runtime::DispatchResult {
		MockStakingInfo::<T>::try_debit_stake(account_id, amount)
			.map(|_| ())
			.ok_or(ERROR_INSUFFICIENT_LIQUIDITY)
	}
}
