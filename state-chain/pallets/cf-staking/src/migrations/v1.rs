use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let accounts = PendingClaims::<T>::iter_keys().drain().collect::<Vec<_>>();
		for account in accounts {
			PendingClaims::<T>::insert(account, ());
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		let pending_claims_accounts = PendingClaims::<T>::iter_keys().collect::<Vec<_>>();
		let num_claims = pending_claims_accounts.len() as u32;
		Ok((pending_claims_accounts, num_claims).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
		let (pending_claims_accounts, num_claims) =
			<(Vec<AccountId<T>>, u32)>::decode(&mut &state[..])
				.map_err(|_| "Failed to decode pre-upgrade state.")?;
		ensure!(num_claims == pending_claims_accounts.len() as u32, "Claims mismatch!");
		for account in pending_claims_accounts {
			ensure!(PendingClaims::<T>::get(account).is_some(), "Missing claim!");
		}
		Ok(())
	}
}
