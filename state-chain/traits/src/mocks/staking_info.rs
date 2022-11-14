#[macro_export]
macro_rules! impl_mock_staking_info {
	($account_id:ty, $balance:ty) => {
		pub struct MockStakingInfo;
		impl StakingInfo for MockStakingInfo {
			type AccountId = $account_id;

			type Balance = $balance;

			fn total_stake_of(_: &Self::AccountId) -> Self::Balance {
				Self::Balance::from(10_u32)
			}

			fn total_onchain_stake() -> Self::Balance {
				Self::Balance::default()
			}
		}
	};
}
