use crate::*;
use cf_traits::GetMinimumFunding;
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use frame_system::RawOrigin;

pub struct Migration<T: Config>(PhantomData<T>);

// A delegation is considered "dust" when the effective bid the auction would
// see — `min(stored_max_bid, balance)` — falls below `MinimumFunding`. The
// stored `max_bid` may legitimately exceed `balance` (e.g. balance dropped via
// redemption after delegation), so we check the effective value rather than
// the stored one. We do not rewrite `max_bid`; the auction code already clamps
// at use time.
impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let min_funding = T::MinimumFunding::get_min_funding_amount();
		let mut total_read = 0u64;
		let mut to_undelegate = Vec::new();

		for (delegator, (_, max_bid)) in DelegationChoice::<T>::iter() {
			total_read += 1;
			let balance = T::FundingInfo::balance(&delegator);
			let effective_bid = core::cmp::min(max_bid, balance);
			if effective_bid.into() < min_funding {
				to_undelegate.push(delegator);
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
			.reads(total_read.saturating_mul(2))
			.saturating_add(T::ValidatorWeightInfo::undelegate().saturating_mul(removed_count))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let min_funding = T::MinimumFunding::get_min_funding_amount();
		let dust_count = DelegationChoice::<T>::iter()
			.filter(|(delegator, (_, max_bid))| {
				let balance = T::FundingInfo::balance(delegator);
				core::cmp::min(*max_bid, balance).into() < min_funding
			})
			.count() as u32;
		Ok(dust_count.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_dust_count =
			u32::decode(&mut &state[..]).map_err(|_| "Failed to decode pre_upgrade state")?;
		let min_funding = T::MinimumFunding::get_min_funding_amount();
		let remaining_dust = DelegationChoice::<T>::iter()
			.filter(|(delegator, (_, max_bid))| {
				let balance = T::FundingInfo::balance(delegator);
				core::cmp::min(*max_bid, balance).into() < min_funding
			})
			.count();
		ensure!(remaining_dust == 0, "Expected no dust delegations after upgrade");
		log::info!(
			target: "cf-validator",
			"remove_underfunded_delegations: removed {} dust delegations",
			pre_dust_count,
		);
		Ok(())
	}
}
