use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::traits::One;
use sp_std::{marker::PhantomData, ops::AddAssign};

use frame_support::{storage, StorageHasher, Twox64Concat};

use crate::CeremonyIdProvider;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockCeremonyIdProvider<Id>(PhantomData<Id>);

impl<Id: Encode + Decode + Default> MockCeremonyIdProvider<Id> {
	const STORAGE_KEY: &'static [u8] = b"MockCeremonyIdProvider::Counter";

	pub fn set(id: Id) {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, Self::STORAGE_KEY, &id)
	}

	pub fn get() -> Id {
		storage::hashed::get_or_default(&<Twox64Concat as StorageHasher>::hash, Self::STORAGE_KEY)
	}
}

impl<Id> CeremonyIdProvider for MockCeremonyIdProvider<Id>
where
	Id: Encode + Decode + Default + Copy + One + AddAssign,
{
	type CeremonyId = Id;

	fn next_ceremony_id() -> Self::CeremonyId {
		let mut id = Self::get();
		id += One::one();
		Self::set(id);
		id
	}
}
