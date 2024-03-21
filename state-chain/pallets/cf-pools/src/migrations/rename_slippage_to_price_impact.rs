use crate::*;
use frame_support::{migration::move_prefix, traits::OnRuntimeUpgrade};
pub struct Migration<T>(PhantomData<T>);

mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type MaximumRelativeSlippage<T: Config> = StorageValue<Pallet<T>, u32, OptionQuery>;
}

impl<T: pallet::Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		// let price_impact = old::MaximumRelativeSlippage::get();
		// MaximumRelativePriceImpact::set(price_impact);
		// old::MaximumRelativeSlippage::<T>::move_prefix(
		// 	b"MaximumRelativeSlippage",
		// 	b"MaximumRelativePriceImpact",
		// );
		move_prefix(b"MaximumRelativeSlippage", b"MaximumRelativePriceImpact");
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let slippage = old::MaximumRelativeSlippage::<T>::get();

		Ok(slippage.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let slippage = <u32>::decode(&mut &state[..]).expect("Pre-migration should encode a u32.");
		ensure!(
			MaximumRelativePriceImpact::<T>::get() == Some(slippage),
			"DepositChannelLookup migration failed."
		);
		Ok(())
	}
}
