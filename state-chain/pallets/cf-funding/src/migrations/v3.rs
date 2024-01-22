use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;

/// Runtime Migration for migrating from V2 to V3: updating PendingRedemption to store to also take
/// the restricted amount into account.
pub struct Migration<T: Config>(PhantomData<T>);

mod old {

	use super::*;

	use frame_support::{pallet_prelude::OptionQuery, Blake2_128Concat};

	#[frame_support::storage_alias]
	pub type PendingRedemptions<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		AccountId<T>,
		(FlipBalance<T>, EthereumAddress),
		OptionQuery,
	>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		old::PendingRedemptions::<T>::drain().for_each(|(key, value)| {
			PendingRedemptions::<T>::insert(
				key,
				// Note: It's not really possible to figure out what the restricted amount was -
				// thats the point of the fix. If face another of these scenarios we have to fix is
				// manually.
				PendingRedemptionInfo {
					total: value.0,
					restricted: value.0,
					redeem_address: value.1,
				},
			);
		});
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let number_pending_redemptions =
			old::PendingRedemptions::<T>::iter_keys().collect::<Vec<_>>().len() as u32;
		Ok(number_pending_redemptions.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let number_pending_redemptions =
			<u32>::decode(&mut &state[..]).map_err(|_| "Failed to decode pre-upgrade state.")?;
		ensure!(
			number_pending_redemptions ==
				PendingRedemptions::<T>::iter_keys().collect::<Vec<_>>().len() as u32,
			"Redemptions mismatch!"
		);
		Ok(())
	}
}

#[cfg(test)]
mod test_runtime_upgrade {
	use super::*;
	use mock::Test;

	#[test]
	fn test() {
		let account_id: <mock::Test as frame_system::Config>::AccountId = [0; 32].into();
		mock::new_test_ext().execute_with(|| {
			let ethereum_address = EthereumAddress::from_slice(&[0; 20]);
			// pre upgrade
			old::PendingRedemptions::<Test>::insert(account_id.clone(), (1, ethereum_address));

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test>::pre_upgrade().unwrap();

			// upgrade
			Migration::<Test>::on_runtime_upgrade();

			// post upgrade
			#[cfg(feature = "try-runtime")]
			Migration::<Test>::post_upgrade(state).unwrap();

			assert_eq!(
				PendingRedemptions::<Test>::get(account_id).unwrap(),
				PendingRedemptionInfo { total: 1, restricted: 1, redeem_address: ethereum_address },
				"Redemption incorrect, should be Pending"
			);
		});
	}
}
