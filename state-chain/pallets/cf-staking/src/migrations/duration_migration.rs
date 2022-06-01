use crate::*;
use frame_support::{
	storage::migration::{put_storage_value, take_storage_value},
	traits::Get,
};
use sp_std::marker::PhantomData;

type OldDuration = (u64, u32);
type NewDuration = u64;
type AccountId<T> = <T as frame_system::Config>::AccountId;
type OldClaimExpiries<T> = Vec<(OldDuration, AccountId<T>)>;
type NewClaimExpiries<T> = Vec<(NewDuration, AccountId<T>)>;

/// Migration from (u64. u32) Duration to u64.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let new_claimttl =
			take_storage_value::<OldDuration>(b"Staking", b"ClaimTTL", b"").unwrap().0;
		let mut new_claim_expiries: NewClaimExpiries<T> = Vec::new();
		take_storage_value::<OldClaimExpiries<T>>(b"Staking", b"ClaimExpiries", b"")
			.unwrap()
			.into_iter()
			.for_each(|(old_duration, account_id)| {
				new_claim_expiries.push((old_duration.0, account_id));
			});

		put_storage_value(b"Staking", b"ClaimTTL", b"", new_claimttl);
		put_storage_value(b"Staking", b"ClaimExpiries", b"", new_claim_expiries);
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
		assert!(get_storage_value::<NewDuration>(b"Staking", b"ClaimTTL", b"").is_some());
		assert!(
			get_storage_value::<NewClaimExpiries<T>>(b"Staking", b"ClaimExpiries", b"").is_some()
		);
		Ok(())
	}
}
