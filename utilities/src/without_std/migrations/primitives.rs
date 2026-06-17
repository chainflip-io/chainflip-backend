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

// --------- primitives --------

use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use crate::migrations::{
	basics::{IdentityMigration, Migration, Version},
	HasChangelog, HasGenericVariant, IsHistoricalType, OrdMigrations,
};

// ----------- identity migrations -------------

macro_rules! impl_identity_migrations {
	($($ty:ty, )*) => {

        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl IsHistoricalType for Type {
            type GetCurrentType = Self;
        }
        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl HasGenericVariant for Type {
            type GenericType = Type;
            type MigrationFromGeneric = IdentityMigration;
        }
        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl HasChangelog for Type {
            type if_unspecified = IdentityMigration;
        }
    };
}

impl_identity_migrations! {(), u8, u128, }

// ----------- wrapped types -------------

pub struct WrapMigration;

macro_rules! impl_identity_migrations_with_wrapper {
	(
        $(#[$meta:meta])*
        struct $Wrapper:ident ( $Ty:ty ) where $(|$var:ident: $Inner:ty| $ctr:expr)?;
    ) => {
		#[derive(
			codec::Encode,
			codec::Decode,
			serde::Serialize,
			serde::Deserialize,
			Clone,
			Debug,
		)]
        $(#[$meta])*
		pub struct $Wrapper(pub $Ty);

		impl scale_info::TypeInfo for $Wrapper {
			type Identity = <$Ty as scale_info::TypeInfo>::Identity;

			fn type_info() -> scale_info::Type {
				<$Ty as scale_info::TypeInfo>::type_info()
			}
		}

		impl Migration<$Ty, crate::migrations::vCurrent> for WrapMigration {
			type From = $Wrapper;

			fn forwards(x: Self::From) -> $Ty {
				x.0
			}

			fn backwards(x: $Ty) -> Self::From {
				$Wrapper(x)
			}
		}

		impl IsHistoricalType for $Wrapper {
			type GetCurrentType = $Ty;
		}
		impl HasGenericVariant for $Ty {
            type GenericType = $Wrapper;
			type MigrationFromGeneric = WrapMigration;
		}
		impl HasChangelog for $Ty {
			type if_unspecified = IdentityMigration;
		}

		$(
			#[cfg(all(feature = "proptest", feature = "std"))]
			impl proptest::arbitrary::Arbitrary for $Wrapper {
				type Parameters = <$Inner as proptest::prelude::Arbitrary>::Parameters;

				fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
					use proptest::prelude::*;
					<$Inner as Arbitrary>::arbitrary_with(args).prop_map(|$var| $Wrapper($ctr))
				}

				type Strategy = impl proptest::strategy::Strategy<Value = Self>;
			}
		)?
	};
}

impl_identity_migrations_with_wrapper! {
	struct WrappedAccountId32(sp_core::crypto::AccountId32) where |x: [u8; 32]| sp_core::crypto::AccountId32::new(x);
}

impl_identity_migrations_with_wrapper! {
	#[derive(PartialOrd, PartialEq, Eq, Ord)]
	struct WrappedH160(sp_core::H160) where |x: [u8; 20]| x.into();
}

impl_identity_migrations_with_wrapper! {
	#[derive(PartialOrd, PartialEq, Eq, Ord, Default)]
	struct WrappedPermill(sp_arithmetic::Permill) where |x: u32| sp_arithmetic::Permill::from_parts(x);
}

// ----------- simple migration that introduces a new type -------------

#[derive(codec::Encode, codec::Decode, scale_info::TypeInfo, PartialEq, Debug)]
#[cfg_attr(all(feature = "proptest", feature = "std"), derive(proptest_derive::Arbitrary))]
pub struct HistoricalEmptyPlaceholder<T>(sp_std::marker::PhantomData<T>);
impl<T: HasGenericVariant + HasChangelog> IsHistoricalType for HistoricalEmptyPlaceholder<T> {
	type GetCurrentType = T;
}

pub struct NewTypeWithDefault;
impl<V: Version, T: HasChangelog + Default> Migration<T, V> for NewTypeWithDefault {
	type From = HistoricalEmptyPlaceholder<T>;

	fn forwards(_: Self::From) -> T {
		Default::default()
	}

	fn backwards(_: T) -> Self::From {
		HistoricalEmptyPlaceholder(Default::default())
	}
}

// ----------- containers -------------

pub struct MapMigration<X>(X);
impl<A, V: Version, M: Migration<A, V>> Migration<Option<A>, V> for MapMigration<M> {
	type From = Option<M::From>;

	fn forwards(x: Self::From) -> Option<A> {
		x.map(M::forwards)
	}

	fn backwards(x: Option<A>) -> Self::From {
		x.map(M::backwards)
	}
}

impl<X: HasChangelog> HasChangelog for Option<X> {
	type if_unspecified = MapMigration<X::if_unspecified>;

	// these have to be specified as otherwise the above
	// default migration doesn't go through. Because rust
	// is forced to work with arbitrary implementations for these,
	// and so can't prove that the historical types are actually
	// all of the shape `Option<...>`.
	type in_20000 = MapMigration<X::in_20000>;
	type in_20100 = MapMigration<X::in_20100>;
	type in_20200 = MapMigration<X::in_20200>;
}
impl<X: HasGenericVariant> HasGenericVariant for Option<X> {
	type GenericType = Option<X::GenericType>;
	type MigrationFromGeneric = MapMigration<X::MigrationFromGeneric>;
}
impl<X: IsHistoricalType> IsHistoricalType for Option<X> {
	type GetCurrentType = Option<X::GetCurrentType>;
}

impl<A, V: Version, M: Migration<A, V>> Migration<Vec<A>, V> for MapMigration<M> {
	type From = Vec<M::From>;

	fn forwards(x: Self::From) -> Vec<A> {
		x.into_iter().map(M::forwards).collect()
	}

	fn backwards(x: Vec<A>) -> Self::From {
		x.into_iter().map(M::backwards).collect()
	}
}

impl<X: HasChangelog> HasChangelog for Vec<X> {
	type if_unspecified = MapMigration<X::if_unspecified>;

	// these have to be specified as otherwise the above
	// default migration doesn't go through. Because rust
	// is forced to work with arbitrary implementations for these,
	// and so can't prove that the historical types are actually
	// all of the shape `Vec<...>`.
	type in_20000 = MapMigration<X::in_20000>;
	type in_20100 = MapMigration<X::in_20100>;
	type in_20200 = MapMigration<X::in_20200>;
}
impl<X: HasGenericVariant> HasGenericVariant for Vec<X> {
	type GenericType = Vec<X::GenericType>;
	type MigrationFromGeneric = MapMigration<X::MigrationFromGeneric>;
}
impl<X: IsHistoricalType> IsHistoricalType for Vec<X> {
	type GetCurrentType = Vec<X::GetCurrentType>;
}

// btreemap

impl<
		A: Ord,
		B,
		V: Version,
		M1: Migration<A, V, From: IsHistoricalType<GetCurrentType: OrdMigrations + Ord> + Ord>,
		M2: Migration<B, V>,
	> Migration<BTreeMap<A, B>, V> for MapMigration<(M1, M2)>
{
	type From = BTreeMap<M1::From, M2::From>;

	fn forwards(x: Self::From) -> BTreeMap<A, B> {
		x.into_iter().map(|(a, b)| (M1::forwards(a), M2::forwards(b))).collect()
	}

	fn backwards(x: BTreeMap<A, B>) -> Self::From {
		x.into_iter().map(|(a, b)| (M1::backwards(a), M2::backwards(b))).collect()
	}
}

impl<A: OrdMigrations + Ord, B: HasChangelog> HasChangelog for BTreeMap<A, B> {
	type if_unspecified = MapMigration<(A::if_unspecified, B::if_unspecified)>;

	// these have to be specified as otherwise the above
	// default migration doesn't go through. Because rust
	// is forced to work with arbitrary implementations for these,
	// and so can't prove that the historical types are actually
	// all of the shape `BTreeMap<...>`.
	type in_20000 = MapMigration<(A::in_20000, B::in_20000)>;
	type in_20100 = MapMigration<(A::in_20100, B::in_20100)>;
	type in_20200 = MapMigration<(A::in_20200, B::in_20200)>;
}
impl<A: HasGenericVariant + Ord, B: HasGenericVariant> HasGenericVariant for BTreeMap<A, B>
where
	A: HasGenericVariant<GenericType: Ord + IsHistoricalTypeOrd>,
{
	type GenericType = BTreeMap<A::GenericType, B::GenericType>;
	type MigrationFromGeneric = MapMigration<(A::MigrationFromGeneric, B::MigrationFromGeneric)>;
}
impl<A: IsHistoricalType<GetCurrentType: OrdMigrations + Ord>, B: IsHistoricalType> IsHistoricalType
	for BTreeMap<A, B>
{
	type GetCurrentType = BTreeMap<A::GetCurrentType, B::GetCurrentType>;
}

trait IsHistoricalTypeOrd = IsHistoricalType<GetCurrentType: OrdMigrations + Ord>;

// tuple (A, B)

impl<A, B, V: Version, M1: Migration<A, V>, M2: Migration<B, V>> Migration<(A, B), V>
	for MapMigration<(M1, M2)>
{
	type From = (M1::From, M2::From);

	fn forwards(x: Self::From) -> (A, B) {
		(M1::forwards(x.0), M2::forwards(x.1))
	}

	fn backwards(x: (A, B)) -> Self::From {
		(M1::backwards(x.0), M2::backwards(x.1))
	}
}

impl<A: HasChangelog, B: HasChangelog> HasChangelog for (A, B) {
	type if_unspecified = MapMigration<(A::if_unspecified, B::if_unspecified)>;

	type in_20000 = MapMigration<(A::in_20000, B::in_20000)>;
	type in_20100 = MapMigration<(A::in_20100, B::in_20100)>;
	type in_20200 = MapMigration<(A::in_20200, B::in_20200)>;
}
impl<A: HasGenericVariant, B: HasGenericVariant> HasGenericVariant for (A, B) {
	type GenericType = (A::GenericType, B::GenericType);
	type MigrationFromGeneric = MapMigration<(A::MigrationFromGeneric, B::MigrationFromGeneric)>;
}
impl<A: IsHistoricalType, B: IsHistoricalType> IsHistoricalType for (A, B) {
	type GetCurrentType = (A::GetCurrentType, B::GetCurrentType);
}
