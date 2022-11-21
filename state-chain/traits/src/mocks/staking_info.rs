use crate::{Chainflip, StakingInfo};
use sp_std::marker::PhantomData;

pub struct MockStakingInfo<T>(PhantomData<T>);

impl<T: Chainflip> StakingInfo for MockStakingInfo<T> {
	type AccountId = T::AccountId;
	type Balance = T::Amount;

	fn total_stake_of(_: &Self::AccountId) -> Self::Balance {
		Self::Balance::from(10_u32)
	}

	fn total_onchain_stake() -> Self::Balance {
		Self::Balance::default()
	}
}
