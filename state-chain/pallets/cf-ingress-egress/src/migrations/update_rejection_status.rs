use crate::{
	Config, TransactionRejectionStatus, TransactionsMarkedForRejection, MARKED_TX_EXPIRATION_BLOCKS,
};
use core::marker::PhantomData;
use frame_support::{sp_runtime::Saturating, traits::UncheckedOnRuntimeUpgrade, weights::Weight};
use frame_system::pallet_prelude::BlockNumberFor;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

mod old {
	#[derive(codec::Encode, codec::Decode)]
	pub enum PrewitnessedStatus {
		Prewitnessed,
		Unseen,
	}
}

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let current_block = frame_system::Pallet::<T>::block_number();
		let expires_at =
			current_block.saturating_add(BlockNumberFor::<T>::from(MARKED_TX_EXPIRATION_BLOCKS));
		TransactionsMarkedForRejection::<T, I>::translate::<old::PrewitnessedStatus, _>(
			|_, _, status| {
				Some(TransactionRejectionStatus {
					prewitnessed: match status {
						old::PrewitnessedStatus::Prewitnessed => true,
						old::PrewitnessedStatus::Unseen => false,
					},
					expires_at,
				})
			},
		);

		Default::default()
	}
}
