use frame_support::traits::Get;
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData};

use crate::Chainflip;

frame_support::generate_storage_alias!(
	Test, KeygenExclusion<T: Chainflip> => Value<BTreeSet<T::ValidatorId>>
);

pub struct MockKeygenExclusion<T>(PhantomData<T>);

impl<T: Chainflip> MockKeygenExclusion<T> {
	pub fn set(ids: Vec<T::ValidatorId>) {
		KeygenExclusion::<T>::put(BTreeSet::<_>::from_iter(ids));
	}
}

impl<T: Chainflip> Get<BTreeSet<T::ValidatorId>> for MockKeygenExclusion<T> {
	fn get() -> BTreeSet<T::ValidatorId> {
		KeygenExclusion::<T>::get().unwrap_or_default()
	}
}
