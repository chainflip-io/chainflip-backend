use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};

/// Runtime Migration for migrating from V0 to V1 based on perseverance at commit `3f3d1ea0` (branch
/// `release/0.6`).
pub struct Migration<T: Config>(PhantomData<T>);

mod archived {

	use super::*;

	use frame_support::pallet_prelude::ValueQuery;

	// Note this is valid only perseverance.
	pub const REDEMPTION_DELAY_BUFFER_SECONDS: u64 = 80;

	// This is added in 0.7 but then removed in 0.8.
	#[frame_support::storage_alias]
	pub type RedemptionDelayBufferSeconds<T: Config> = StorageValue<Pallet<T>, u64, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let accounts = PendingRedemptions::<T>::iter_keys().drain().collect::<Vec<_>>();
		for account in accounts {
			PendingRedemptions::<T>::insert(account, ());
		}
		archived::RedemptionDelayBufferSeconds::<T>::put(archived::REDEMPTION_DELAY_BUFFER_SECONDS);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		let pending_redemptions_accounts = PendingRedemptions::<T>::iter_keys().collect::<Vec<_>>();
		let num_redemptions = pending_redemptions_accounts.len() as u32;
		Ok((pending_redemptions_accounts, num_redemptions).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
		let (pending_redemptions_accounts, num_redemptions) =
			<(Vec<AccountId<T>>, u32)>::decode(&mut &state[..])
				.map_err(|_| "Failed to decode pre-upgrade state.")?;
		ensure!(
			num_redemptions == pending_redemptions_accounts.len() as u32,
			"Redemptions mismatch!"
		);
		for account in pending_redemptions_accounts {
			ensure!(PendingRedemptions::<T>::get(account).is_some(), "Missing redemption!");
		}
		Ok(())
	}
}
