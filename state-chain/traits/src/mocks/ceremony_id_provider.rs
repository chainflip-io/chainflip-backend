use cf_primitives::CeremonyId;
use codec::{Decode, Encode};
use scale_info::TypeInfo;

use frame_support::{storage, StorageHasher, Twox64Concat};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockCeremonyIdProvider;

impl MockCeremonyIdProvider {
	const STORAGE_KEY: &'static [u8] = b"MockCeremonyIdProvider::Counter";

	pub fn set(id: CeremonyId) {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, Self::STORAGE_KEY, &id)
	}

	pub fn get() -> CeremonyId {
		storage::hashed::get_or_default(&<Twox64Concat as StorageHasher>::hash, Self::STORAGE_KEY)
	}
}
