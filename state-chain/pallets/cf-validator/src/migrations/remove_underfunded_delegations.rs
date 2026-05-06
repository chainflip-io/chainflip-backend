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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use cf_traits::mocks::funding_info::MockFundingInfo;

	#[test]
	fn removes_only_dust_delegations() {
		const MIN_FUNDING: u128 = 100; // matches MockMinimumFundingProvider

		const LOW_MAX_BID: u64 = 200;
		const LOW_BALANCE: u64 = 201;
		const STALE_BUT_FUNDED: u64 = 202;
		const HEALTHY: u64 = 203;
		const EXACTLY_AT_MIN: u64 = 204;

		new_test_ext().execute_with(|| {
			// Pre-existing on-chain state may violate today's invariants (e.g.
			// max_bid < MinimumFunding, or max_bid > balance). Seed those
			// scenarios directly via storage writes — the regular `delegate`
			// path enforces MinimumFunding and clamps to balance.
			let stale_max_bid = MIN_FUNDING * 10;
			let scenarios = [
				(LOW_MAX_BID, MIN_FUNDING * 5, MIN_FUNDING - 1),
				(LOW_BALANCE, MIN_FUNDING - 1, MIN_FUNDING * 5),
				(STALE_BUT_FUNDED, MIN_FUNDING * 2, stale_max_bid),
				(HEALTHY, MIN_FUNDING * 5, MIN_FUNDING * 3),
				(EXACTLY_AT_MIN, MIN_FUNDING * 5, MIN_FUNDING),
			];
			MockFundingInfo::<Test>::set_balances(
				scenarios.iter().map(|(account, balance, _)| (*account, *balance)),
			);
			for (account, _, max_bid) in scenarios {
				DelegationChoice::<Test>::insert(account, (ALICE, max_bid));
			}

			Migration::<Test>::on_runtime_upgrade();

			// Dust entries are removed: effective_bid = min(max_bid, balance)
			// is below MinimumFunding for both LOW_MAX_BID (low max_bid) and
			// LOW_BALANCE (low balance behind a high stored max_bid).
			assert!(DelegationChoice::<Test>::get(LOW_MAX_BID).is_none());
			assert!(DelegationChoice::<Test>::get(LOW_BALANCE).is_none());

			// Non-dust entries are kept.
			assert!(DelegationChoice::<Test>::get(HEALTHY).is_some());
			assert!(DelegationChoice::<Test>::get(EXACTLY_AT_MIN).is_some());

			// Stale max_bid (max_bid > balance, but effective_bid still ≥ min)
			// is kept *and not rewritten* — the migration must not mutate
			// user-set max_bid values.
			assert_eq!(
				DelegationChoice::<Test>::get(STALE_BUT_FUNDED).map(|(_, max_bid)| max_bid),
				Some(stale_max_bid)
			);
		});
	}
}
