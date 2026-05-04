// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

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

pub mod __reexports {
	pub use log;
}

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
			$crate::__reexports::log::error!("log_or_panic: {}", format_args!($($arg)*));
		}
	};
}

#[cfg(test)]
mod test {
	use super::*;
	use codec::{Decode, DecodeWithMemTracking, Encode};
	use frame_support::storage_alias;

	#[storage_alias]
	type Store = StorageValue<Test, MyEnumType>;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking)]
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
	use codec::{Decode, DecodeWithMemTracking, Encode};
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

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, EnumVariant)]
	enum MyEnumType {
		A(u32),
		B(Vec<u8>),
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, EnumVariant)]
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
		hex_literal::hex!("4c5328ad95cedeb3c89e24edd12cb687d950fd3da8559358dc474f0ddd9a3f99");

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

/// Wraps a migration that should run unconditionally on every runtime upgrade, without
/// participating in the version chain. Must be the last element in a `PalletMigration` tuple.
///
/// `AlwaysRunMigration` does not implement `MigrationSequence` on its own; only tuples ending
/// with it do. This ensures at compile time that it appears last.
pub struct AlwaysRunMigration<M: OnRuntimeUpgrade>(PhantomData<M>);

impl<M: OnRuntimeUpgrade> OnRuntimeUpgrade for AlwaysRunMigration<M> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		M::on_runtime_upgrade()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, frame_support::sp_runtime::TryRuntimeError> {
		M::pre_upgrade()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(
		state: sp_std::vec::Vec<u8>,
	) -> Result<(), frame_support::pallet_prelude::DispatchError> {
		M::post_upgrade(state)
	}
}

impl<X: MigrationSequence, M: OnRuntimeUpgrade> MigrationSequence for (X, AlwaysRunMigration<M>) {
	const FROM: u16 = X::FROM;
	const TO: u16 = X::TO;
}

macro_rules! impl_migration_sequence_ending_with_always_run {
	($first:ident, $($rest:ident),+) => {
		impl<$first: MigrationSequence, $($rest: MigrationSequence),+, Inner__: OnRuntimeUpgrade>
			MigrationSequence for ($first, $($rest),+, AlwaysRunMigration<Inner__>)
		where
			($first, $($rest),+): MigrationSequence,
		{
			const FROM: u16 = <($first, $($rest),+) as MigrationSequence>::FROM;
			const TO: u16 = <($first, $($rest),+) as MigrationSequence>::TO;
		}
	};
}

impl_migration_sequence_ending_with_always_run!(A, B);
impl_migration_sequence_ending_with_always_run!(A, B, C);
impl_migration_sequence_ending_with_always_run!(A, B, C, D);
impl_migration_sequence_ending_with_always_run!(A, B, C, D, E);
impl_migration_sequence_ending_with_always_run!(A, B, C, D, E, F);
impl_migration_sequence_ending_with_always_run!(A, B, C, D, E, F, G);

/// Connects the FROM/TO versions of a sequence of VersionedMigrations.
///
/// There is a compile-time check that the sequence is contiguous, i.e. that the TO version of each
/// migration matches the FROM version of the next. The main use-case is the following pattern in a
/// pallet's `migrations.rs`. Accessing the FROM version of the composed migration sequence forces
/// evaluation of the sequence. If the sequence is not contiguous, it triggers a compile-time error:
///
/// ```ignore
/// #[cfg(test)]
/// const _: u16 =
///     <PalletMigration<crate::mocks::Test> as cf_runtime_utilities::MigrationSequence>::FROM;
/// ```
pub trait MigrationSequence {
	const FROM: u16;
	const TO: u16;
}

impl<const AT: u16, P> MigrationSequence for PlaceholderMigration<AT, P>
where
	P: PalletInfoAccess + GetStorageVersion<InCodeStorageVersion = StorageVersion>,
{
	const FROM: u16 = AT;
	const TO: u16 = AT;
}

impl<const MIGRATION_FROM: u16, const MIGRATION_TO: u16, Inner, Pallet, Weight> MigrationSequence
	for frame_support::migrations::VersionedMigration<
		MIGRATION_FROM,
		MIGRATION_TO,
		Inner,
		Pallet,
		Weight,
	>
{
	const FROM: u16 = MIGRATION_FROM;
	const TO: u16 = MIGRATION_TO;
}

impl<A: MigrationSequence> MigrationSequence for (A,) {
	const FROM: u16 = A::FROM;
	const TO: u16 = A::TO;
}

macro_rules! impl_migration_sequence_for_tuple {
	($first:ident, $($rest:ident),+ $(,)?) => {
		impl<$first: MigrationSequence, $($rest: MigrationSequence),+> MigrationSequence
			for ($first, $($rest),+)
		{
			const FROM: u16 = {
				impl_migration_sequence_for_tuple!(@checks $first, $($rest,)+);
				$first::FROM
			};
			const TO: u16 = impl_migration_sequence_for_tuple!(@last $($rest),+);
		}
	};

	(@last $only:ident) => { $only::TO };
	(@last $_head:ident, $($rest:ident),+) => {
		impl_migration_sequence_for_tuple!(@last $($rest),+)
	};

	(@checks $prev:ident, $next:ident, $($rest:ident),* $(,)?) => {
		if $prev::TO != $next::FROM {
			panic!(concat!(
				"Migration sequence not contiguous: ",
				stringify!($prev),
				"::TO != ",
				stringify!($next),
				"::FROM",
			));
		}
		impl_migration_sequence_for_tuple!(@checks $next, $($rest,)*);
	};
	(@checks $_last:ident $(,)?) => {};
}

impl_migration_sequence_for_tuple!(A, B);
impl_migration_sequence_for_tuple!(A, B, C);
impl_migration_sequence_for_tuple!(A, B, C, D);
impl_migration_sequence_for_tuple!(A, B, C, D, E);
impl_migration_sequence_for_tuple!(A, B, C, D, E, F);
impl_migration_sequence_for_tuple!(A, B, C, D, E, F, G);
impl_migration_sequence_for_tuple!(A, B, C, D, E, F, G, H);
