use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::dispatch::DispatchError;

/// Runtime Migration for migrating from V0 to V1 based on perseverance at commit `3f3d1ea0` (branch
/// `release/0.6`).
pub struct Migration<T: Config>(PhantomData<T>);

mod old {

	use super::*;

	use frame_support::{pallet_prelude::OptionQuery, Blake2_128Concat};

	// This is added in 0.7 but then removed in 0.8.
	#[frame_support::storage_alias]
	pub type OldPendingRedemptions<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, AccountId<T>, (), OptionQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let accounts = old::OldPendingRedemptions::<T>::iter_keys().drain().collect::<Vec<_>>();
		for account in accounts {
			PendingRedemptions::<T>::insert(account, Pending::Pending);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let pending_redemptions_accounts =
			old::OldPendingRedemptions::<T>::iter_keys().collect::<Vec<_>>();
		let num_redemptions = pending_redemptions_accounts.len() as u32;
		Ok((pending_redemptions_accounts, num_redemptions).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let (pending_redemptions_accounts, num_redemptions) =
			<(Vec<AccountId<T>>, u32)>::decode(&mut &state[..])
				.map_err(|_| "Failed to decode pre-upgrade state.")?;
		ensure!(
			num_redemptions == pending_redemptions_accounts.len() as u32,
			"Redemptions mismatch!"
		);
		for account in pending_redemptions_accounts {
			ensure!(PendingRedemptions::<T>::get(account.clone()).is_some(), "Missing redemption!");
			ensure!(
				PendingRedemptions::<T>::get(account).unwrap() == Pending::Pending,
				"Redemption not containing Pending::Pending!"
			);
		}
		Ok(())
	}
}

#[cfg(test)]
mod test_runtime_upgrade {
	use super::*;
	use crate::migrations::v2::old::OldPendingRedemptions;
	use mock::Test;

	#[test]
	fn test() {
		let account_id: <mock::Test as frame_system::Config>::AccountId = [0; 32].into();

		mock::new_test_ext().execute_with(|| {
			// pre upgrade
			OldPendingRedemptions::<Test>::insert(account_id.clone(), ());

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test>::pre_upgrade().unwrap();

			// upgrade
			Migration::<Test>::on_runtime_upgrade();

			// post upgrade
			#[cfg(feature = "try-runtime")]
			Migration::<Test>::post_upgrade(state).unwrap();

			assert_eq!(
				PendingRedemptions::<Test>::get(account_id).unwrap(),
				Pending::Pending,
				"Redemption incorrect, should be Pending"
			);
		});
	}
}
