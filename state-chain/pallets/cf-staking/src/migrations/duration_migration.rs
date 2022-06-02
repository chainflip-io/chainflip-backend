use crate::*;
use frame_support::{assert_ok, traits::Get};
use sp_std::marker::PhantomData;

type OldDuration = (u64, u32);
type AccountId<T> = <T as frame_system::Config>::AccountId;
type OldClaimExpiries<T> = Vec<(OldDuration, AccountId<T>)>;

/// Migration from (u64. u32) Duration to u64.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		assert_ok!(ClaimExpiries::<T>::translate(
			|old_claim_expiries: Option<OldClaimExpiries<T>>| {
				Some(old_claim_expiries.unwrap().into_iter().map(|(old, id)| (old.0, id)).collect())
			}
		));
		assert_ok!(ClaimTTL::<T>::translate(|old_claimttl: Option<OldDuration>| {
			Some(old_claimttl.unwrap().0)
		}));
		T::DbWeight::get().reads_writes(2, 2)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		use frame_support::storage::migration::get_storage_value;
		assert!(get_storage_value::<OldDuration>(b"Staking", b"ClaimTTL", b"").is_some());
		assert!(
			get_storage_value::<OldClaimExpiries<T>>(b"Staking", b"ClaimExpiries", b"").is_some()
		);
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use frame_support::storage::migration::get_storage_value;
		assert!(get_storage_value::<u64>(b"Staking", b"ClaimTTL", b"").is_some());
		assert!(
			get_storage_value::<NewClaimExpiries<T>>(b"Staking", b"ClaimExpiries", b"").is_some()
		);
		Ok(())
	}
}
