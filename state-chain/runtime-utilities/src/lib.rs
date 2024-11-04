#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(feature = "derive")]
pub use cf_runtime_macros::*;
use codec::FullCodec;

use frame_support::{
	pallet_prelude::GetStorageVersion,
	traits::{OnRuntimeUpgrade, PalletInfoAccess, StorageVersion, UncheckedOnRuntimeUpgrade},
	StorageMap, StorageValue,
};
use sp_std::marker::PhantomData;

mod helper_functions;
pub use helper_functions::*;

pub mod migration_template;

/// Decode the variant of a stored enum.
///
/// May panic if V does not cover all possible variants of the stored enum. Use
/// the [EnumVariant] derive macro to avoid this. See the tests for an example.
pub fn storage_decode_variant<V: EnumVariant>(hashed_key: &[u8]) -> Option<V::Variant> {
	V::from_discriminant(storage_discriminant(hashed_key)?)
}

/// Get the discriminant of a stored enum.
///
/// If the stored value is not an enum, the result will be meaningless.
pub fn storage_discriminant(hashed_key: &[u8]) -> Option<u8> {
	let mut data = [0u8; 1];
	let _ = sp_io::storage::read(hashed_key, &mut data, 0)?;
	Some(data[0])
}

/// Conversion from an enum's discriminant to a stripped-down enum containing
/// just the discriminants.
pub trait EnumVariant {
	type Variant;

	fn from_discriminant(d: u8) -> Option<Self::Variant>;
}

/// Allows us to just decode the variant when that is all we care about.
/// This is useful when it may be expensive to decode the whole variant type.
pub trait StorageDecodeVariant<V: EnumVariant> {
	fn decode_variant() -> Option<V::Variant>;
}

pub trait StorageMapDecodeVariant<K, V: EnumVariant> {
	fn decode_variant_for(key: &K) -> Option<V::Variant>;
}

impl<T, V> StorageDecodeVariant<V> for T
where
	T: StorageValue<V>,
	V: EnumVariant + FullCodec,
{
	fn decode_variant() -> Option<V::Variant> {
		storage_decode_variant::<V>(&T::hashed_key())
	}
}

impl<T, K, V> StorageMapDecodeVariant<K, V> for T
where
	T: StorageMap<K, V>,
	K: FullCodec,
	V: EnumVariant + FullCodec,
{
	fn decode_variant_for(key: &K) -> Option<V::Variant> {
		storage_decode_variant::<V>(&T::hashed_key_for(key))
	}
}

/// Logs if running in release, panics if running in test.
#[macro_export]
macro_rules! log_or_panic {
    ($($arg:tt)*) => {
		if cfg!(debug_assertions) {
			use scale_info::prelude::format;
            panic!("log_or_panic: {}", format_args!($($arg)*));
		} else {
			use scale_info::prelude::format;
            log::error!("log_or_panic: {}", format_args!($($arg)*));
		}
    };
}

#[cfg(test)]
mod test {
	use super::*;
	use codec::{Decode, Encode};
	use frame_support::storage_alias;

	#[storage_alias]
	type Store = StorageValue<Test, MyEnumType>;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	enum MyEnumType {
		A(u32),
		B(Vec<u8>),
	}

	#[test]
	fn test_storage_discriminant() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			Store::put(MyEnumType::A(42));
			assert_eq!(storage_discriminant(&Store::hashed_key()), Some(0u8));
			Store::put(MyEnumType::B(b"hello".to_vec()));
			assert_eq!(storage_discriminant(&Store::hashed_key()), Some(1u8));
		});
	}
}

#[cfg(feature = "derive")]
#[cfg(test)]
mod test_derive {
	use super::*;
	use codec::{Decode, Encode};
	use frame_support::{storage_alias, Twox64Concat};

	#[storage_alias]
	type ValueStore = StorageValue<Test, MyEnumType>;

	trait Config {
		type Inner: FullCodec;
	}

	struct TestConfig;

	impl Config for TestConfig {
		type Inner = u32;
	}

	#[storage_alias]
	type MapStore<T> = StorageMap<Pallet, Twox64Concat, u32, MyGenericEnumType<T>>;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, EnumVariant)]
	enum MyEnumType {
		A(u32),
		B(Vec<u8>),
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, EnumVariant)]
	enum MyGenericEnumType<T: Config> {
		A(T::Inner),
		B(T::Inner),
	}

	#[test]
	fn test_storage_value() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			ValueStore::put(MyEnumType::A(42));
			assert_eq!(
				storage_decode_variant::<MyEnumType>(&ValueStore::hashed_key()),
				Some(<MyEnumType as EnumVariant>::Variant::A)
			);
			ValueStore::put(MyEnumType::B(b"hello".to_vec()));
			assert_eq!(
				storage_decode_variant::<MyEnumType>(&ValueStore::hashed_key()),
				Some(<MyEnumType as EnumVariant>::Variant::B)
			);

			// Try the same with the storage traits.
			assert_eq!(ValueStore::decode_variant(), Some(MyEnumTypeVariant::B));
		});
	}

	#[test]
	fn test_storage_map() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MapStore::<TestConfig>::insert(123, MyGenericEnumType::<TestConfig>::A(42));

			assert_eq!(
				MapStore::<TestConfig>::decode_variant_for(&123),
				Some(MyGenericEnumTypeVariant::A)
			);
			assert_eq!(MapStore::<TestConfig>::decode_variant_for(&122), None);
		});
	}
}

pub mod genesis_hashes {
	use frame_support::sp_runtime::traits::Zero;
	use frame_system::pallet_prelude::BlockNumberFor;
	use sp_core::H256;

	pub const BERGHAIN: [u8; 32] =
		hex_literal::hex!("8b8c140b0af9db70686583e3f6bf2a59052bfe9584b97d20c45068281e976eb9");
	pub const PERSEVERANCE: [u8; 32] =
		hex_literal::hex!("7a5d4db858ada1d20ed6ded4933c33313fc9673e5fffab560d0ca714782f2080");
	/// NOTE: IF YOU USE THIS CONSTANT, MAKE SURE IT IS STILL VALID: SISYPHOS IS RELAUNCHED
	/// FROM TIME TO TIME.
	pub const SISYPHOS: [u8; 32] =
		hex_literal::hex!("7db0684f891ad10fa919c801f9a9f030c0f6831aafa105b1a68e47803f91f2b6");

	pub fn genesis_hash<T: frame_system::Config<Hash = H256>>() -> [u8; 32] {
		frame_system::BlockHash::<T>::get(BlockNumberFor::<T>::zero()).to_fixed_bytes()
	}
}

/// A placeholder migration that does nothing. Useful too allow us to keep the boilerplate in the
/// runtime consistent.
pub struct PlaceholderMigration<
	const AT: u16,
	P: PalletInfoAccess + GetStorageVersion<InCodeStorageVersion = StorageVersion>,
>(PhantomData<P>);

impl<const AT: u16, P> OnRuntimeUpgrade for PlaceholderMigration<AT, P>
where
	P: PalletInfoAccess + GetStorageVersion<InCodeStorageVersion = StorageVersion>,
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if <P as GetStorageVersion>::on_chain_storage_version() == AT {
			log::info!(
				"👌 {}: Placeholder migration at pallet storage version {:?}. Nothing to do.",
				P::name(),
				AT,
			);
		} else {
			log::warn!(
				"🚨 {}: Placeholder migration at pallet storage version {:?} but storage version is {:?}.",
				P::name(),
				AT,
				<P as GetStorageVersion>::on_chain_storage_version(),
			);
		}
		Default::default()
	}
}

pub struct NoopRuntimeUpgrade;

impl UncheckedOnRuntimeUpgrade for NoopRuntimeUpgrade {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Default::default()
	}
}
