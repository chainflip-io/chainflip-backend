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

		for (delegator, (_, max_bid)) in DelegationChoice::<T>::iter() {
			total_read += 1;
			if max_bid.into() < min_funding {
				to_undelegate.push(delegator);
			}
		}

		let removed_count = to_undelegate.len() as u64;
		for delegator in to_undelegate {
			if let Err(e) = Pallet::<T>::undelegate(
				RawOrigin::Signed(delegator).into(),
				DelegationAmount::Max,
			) {
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
		let count = DelegationChoice::<T>::iter()
			.filter(|(_, (_, max_bid))| Into::<u128>::into(*max_bid) < min_funding)
			.count() as u32;
		Ok(count.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_count = u32::decode(&mut &state[..])
			.map_err(|_| "Failed to decode pre_upgrade state")?;
		let min_funding = T::MinimumFunding::get_min_funding_amount();
		let remaining = DelegationChoice::<T>::iter()
			.filter(|(_, (_, max_bid))| Into::<u128>::into(*max_bid) < min_funding)
			.count();
		ensure!(remaining == 0, "Expected all underfunded delegations to be removed");
		log::info!(
			target: "cf-validator",
			"remove_underfunded_delegations: removed {} underfunded delegations",
			pre_count,
		);
		Ok(())
	}
}
