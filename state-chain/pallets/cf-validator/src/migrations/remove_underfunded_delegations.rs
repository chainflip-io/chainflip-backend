use crate::*;
use cf_traits::GetMinimumFunding;
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use frame_system::RawOrigin;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let min_funding = T::MinimumFunding::get_min_funding_amount();
		let mut total_read = 0u64;
		let mut to_undelegate = Vec::new();

		for (delegator, (validator, mut max_bid)) in DelegationChoice::<T>::iter() {
			total_read += 1;

			let balance = T::FundingInfo::balance(&delegator);
			if max_bid > balance {
				log::info!(
					target: "cf-validator",
					"max_bid should be lower or equal to balance. violation found and so setting max_bid equal to balance"
				);
				max_bid = balance;
				DelegationChoice::<T>::insert(&delegator, (validator, max_bid));
			}

			if max_bid.into() < min_funding {
				to_undelegate.push(delegator.clone());
			}
		}

		let removed_count = to_undelegate.len() as u64;
		for delegator in to_undelegate {
			if let Err(e) =
				Pallet::<T>::undelegate(RawOrigin::Signed(delegator).into(), DelegationAmount::Max)
			{
				log::error!(
					target: "cf-validator",
					"remove_underfunded_delegations: failed to undelegate: {:?}",
					e
				);
			}
		}

		T::DbWeight::get()
			.reads(total_read)
			.saturating_add(T::ValidatorWeightInfo::undelegate().saturating_mul(removed_count))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let min_funding = T::MinimumFunding::get_min_funding_amount();
		let over_balance_count = DelegationChoice::<T>::iter()
			.filter(|(delegator, (_, max_bid))| {
				let balance = T::FundingInfo::balance(delegator);
				*max_bid > balance
			})
			.count() as u32;
		let underfunded_count = DelegationChoice::<T>::iter()
			.filter(|(delegator, (_, max_bid))| {
				let balance = T::FundingInfo::balance(delegator);
				Into::<u128>::into(*max_bid) < min_funding || balance.into() < min_funding
			})
			.count() as u32;
		Ok((over_balance_count, underfunded_count).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let (pre_over_balance, pre_underfunded) =
			<(u32, u32)>::decode(&mut &state[..]).map_err(|_| "Failed to decode pre_upgrade state")?;
		let min_funding = T::MinimumFunding::get_min_funding_amount();
		let remaining_over_balance = DelegationChoice::<T>::iter()
			.filter(|(delegator, (_, max_bid))| {
				let balance = T::FundingInfo::balance(delegator);
				*max_bid > balance
			})
			.count();
		let remaining_underfunded = DelegationChoice::<T>::iter()
			.filter(|(delegator, (_, max_bid))| {
				let balance = T::FundingInfo::balance(delegator);
				Into::<u128>::into(*max_bid) < min_funding || balance.into() < min_funding
			})
			.count();
		ensure!(remaining_over_balance == 0, "Expected no max_bid values above balance after upgrade");
		ensure!(remaining_underfunded == 0, "Expected no underfunded delegations after upgrade");
		log::info!(
			target: "cf-validator",
			"remove_underfunded_delegations: pre-upgrade over_balance={}, underfunded={}; post-upgrade over_balance={}, underfunded={}",
			pre_over_balance,
			pre_underfunded,
			remaining_over_balance,
			remaining_underfunded,
		);
		Ok(())
	}
}
