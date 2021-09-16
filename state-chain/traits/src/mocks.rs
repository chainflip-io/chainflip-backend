#![cfg(feature = "std")]

use frame_support::traits::EnsureOrigin;
use sp_std::marker::PhantomData;
pub mod ensure_governance;
pub mod ensure_witnessed;
pub mod epoch_info;
pub mod stake_transfer;
pub mod time_source;
pub mod vault_rotation;
pub mod witnesser;

pub struct NeverFailingOriginCheck<T>(PhantomData<T>);

impl<T: frame_system::Config> EnsureOrigin<T> for NeverFailingOriginCheck<T> {
	type Success = ();

	fn try_origin(_o: T) -> std::result::Result<Self::Success, T> {
		Ok(())
	}
}
