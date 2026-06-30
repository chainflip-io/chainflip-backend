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

use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, vec::Vec};

use crate::{
	migrations::{
		basics::{IdentityMigration, Migration, Version},
		with_all_runtime_migrations, HasChangelog, HasGenericVariant, IsHistoricalType,
		OrdMigrations,
	},
	never::Never,
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

impl_identity_migrations! {(), u8, u16, u128, }

impl<T> IsHistoricalType for PhantomData<T> {
	type GetCurrentType = Self;
}

impl<T> HasGenericVariant for PhantomData<T> {
	type GenericType = Self;
	type MigrationFromGeneric = IdentityMigration;
}

impl<T> HasChangelog for PhantomData<T> {
	type if_unspecified = IdentityMigration;
}

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
			type ForwardsError = Never;
			type BackwardsError = Never;

			fn forwards<E: From<Self::ForwardsError>>(x: Self::From) -> Result<$Ty, E> {
				Ok(x.0)
			}

			fn backwards<E: From<Self::BackwardsError>>(x: $Ty) -> Result<Self::From, E> {
				Ok($Wrapper(x))
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
	type ForwardsError = Never;
	type BackwardsError = Never;

	fn forwards<E: From<Self::ForwardsError>>(_: Self::From) -> Result<T, E> {
		Ok(Default::default())
	}

	fn backwards<E: From<Self::BackwardsError>>(_: T) -> Result<Self::From, E> {
		Ok(HistoricalEmptyPlaceholder(Default::default()))
	}
}

// ----------- containers -------------

pub struct MapMigration<X>(X);

pub enum MapMigrationForwardsError<A, B> {
	First(A),
	Second(B),
}

pub enum MapMigrationBackwardsError<A, B> {
	First(A),
	Second(B),
}

macro_rules! impl_migrations_for_container {
    (
        $container:ident<$($ty:ident $(: ($($ty_path:tt)*))?),+>,
        $container_macro:ident,
		[$var_M:ident $(where From: ($($from_path:tt)*) )?],
        |$var_f:ident| $expr_f:expr,
        |$var_b:ident| $expr_b:expr,
    ) => {
        macro_rules! $container_macro {
            ($$($$migration:ident, )*) => {
				impl<$($ty: HasChangelog $(+ $($ty_path)*)?),+> HasChangelog for $container<$($ty),+> {
					type if_unspecified = MapMigration<( $($ty::if_unspecified, )+ )>;

                    $$(
                        type $$migration = MapMigration<( $($ty::$$migration, )+ )>;
                    )*
                }
            }
        }
        with_all_runtime_migrations!{ $container_macro }

		impl<$($ty $(: $($ty_path)* )? ,)+  V: Version, $var_M: Migration<$($ty),+, V $(, From: $($from_path)*)?>> Migration<$container<$($ty),+>, V> for MapMigration<($var_M,)> {
			type From = $container<$var_M::From>;
			type ForwardsError = $var_M::ForwardsError;
			type BackwardsError = $var_M::BackwardsError;

			fn forwards<E: From<Self::ForwardsError>>($var_f: Self::From) -> Result<$container<$($ty),+>, E> {
                $expr_f
            }

			fn backwards<E: From<Self::BackwardsError>>($var_b: $container<$($ty),+>) -> Result<Self::From, E> {
                $expr_b
            }
        }

		impl<$($ty: HasGenericVariant $(+ $($ty_path)*)?),+ > HasGenericVariant for $container<$($ty),+> {
            type GenericType = $container<$($ty::GenericType),+>;
            type MigrationFromGeneric = MapMigration<($($ty::MigrationFromGeneric, )+)>;
        }
		impl<$($ty: IsHistoricalType $(+ $($ty_path)*)?),+> IsHistoricalType for $container<$($ty),+> {
            type GetCurrentType = $container<$($ty::GetCurrentType),+>;
        }
    };
}

impl_migrations_for_container! {
	Option<X>,
	impl_changelog_for_option,
	[M],
	|x| x.map(M::forwards).transpose(),
	|x| x.map(M::backwards).transpose(),
}

impl_migrations_for_container! {
	Vec<X>,
	impl_changelog_for_vector,
	[M],
	|x| x.into_iter().map(M::forwards).collect(),
	|x| x.into_iter().map(M::backwards).collect(),
}

pub type TupleWith1Entry<A> = (A,);

impl_migrations_for_container! {
	TupleWith1Entry<A>,
	impl_changelog_for_tuple1,
	[M1],
	|x| Ok((M1::forwards(x.0)?,)),
	|x| Ok((M1::backwards(x.0)?,)),
}

pub type TupleWith2Entries<A, B> = (A, B);

macro_rules! impl_changelog_for_tuple {
    ($($migration:ident,)*) => {
		impl<A: HasChangelog, B: HasChangelog> HasChangelog for TupleWith2Entries<A, B> {
            type if_unspecified = MapMigration<(A::if_unspecified, B::if_unspecified)>;

            $(
                type $migration = MapMigration<(A::$migration, B::$migration)>;
            )*
        }
    };
}
with_all_runtime_migrations! {impl_changelog_for_tuple}

impl<A, B, V: Version, M1: Migration<A, V>, M2: Migration<B, V>>
	Migration<TupleWith2Entries<A, B>, V> for MapMigration<(M1, M2)>
{
	type From = TupleWith2Entries<M1::From, M2::From>;
	type ForwardsError = MapMigrationForwardsError<M1::ForwardsError, M2::ForwardsError>;
	type BackwardsError = MapMigrationBackwardsError<M1::BackwardsError, M2::BackwardsError>;

	fn forwards<E: From<Self::ForwardsError>>(x: Self::From) -> Result<TupleWith2Entries<A, B>, E> {
		Ok((
			M1::forwards::<M1::ForwardsError>(x.0)
				.map_err(MapMigrationForwardsError::First)
				.map_err(E::from)?,
			M2::forwards::<M2::ForwardsError>(x.1)
				.map_err(MapMigrationForwardsError::Second)
				.map_err(E::from)?,
		))
	}

	fn backwards<E: From<Self::BackwardsError>>(
		x: TupleWith2Entries<A, B>,
	) -> Result<Self::From, E> {
		Ok((
			M1::backwards::<M1::BackwardsError>(x.0)
				.map_err(MapMigrationBackwardsError::First)
				.map_err(E::from)?,
			M2::backwards::<M2::BackwardsError>(x.1)
				.map_err(MapMigrationBackwardsError::Second)
				.map_err(E::from)?,
		))
	}
}

impl<A: HasGenericVariant, B: HasGenericVariant> HasGenericVariant for TupleWith2Entries<A, B> {
	type GenericType = TupleWith2Entries<A::GenericType, B::GenericType>;
	type MigrationFromGeneric = MapMigration<(A::MigrationFromGeneric, B::MigrationFromGeneric)>;
}
impl<A: IsHistoricalType, B: IsHistoricalType> IsHistoricalType for TupleWith2Entries<A, B> {
	type GetCurrentType = TupleWith2Entries<A::GetCurrentType, B::GetCurrentType>;
}

// ---- btreemap ----
// the bounds are quite messy and difficult to replicate with the `impl_migrations_for_container`
// macro, so we use a manual implementation:

macro_rules! impl_changelog_for_btreemap {
    ($($migration:ident,)*) => {
		impl<A: OrdMigrations + Ord, B: HasChangelog> HasChangelog for BTreeMap<A, B> {
            type if_unspecified = MapMigration<(A::if_unspecified, B::if_unspecified)>;

            $(
                type $migration = MapMigration<(A::$migration, B::$migration)>;
            )*
        }
    };
}
with_all_runtime_migrations! {impl_changelog_for_btreemap}

impl<
		A: Ord,
		B,
		V: Version,
		M1: Migration<A, V, From: IsHistoricalType<GetCurrentType: OrdMigrations + Ord> + Ord>,
		M2: Migration<B, V>,
	> Migration<BTreeMap<A, B>, V> for MapMigration<(M1, M2)>
{
	type From = BTreeMap<M1::From, M2::From>;
	type ForwardsError = MapMigrationForwardsError<M1::ForwardsError, M2::ForwardsError>;
	type BackwardsError = MapMigrationBackwardsError<M1::BackwardsError, M2::BackwardsError>;

	fn forwards<E: From<Self::ForwardsError>>(x: Self::From) -> Result<BTreeMap<A, B>, E> {
		x.into_iter()
			.map(|(a, b)| {
				Ok((
					M1::forwards::<M1::ForwardsError>(a)
						.map_err(MapMigrationForwardsError::First)
						.map_err(E::from)?,
					M2::forwards::<M2::ForwardsError>(b)
						.map_err(MapMigrationForwardsError::Second)
						.map_err(E::from)?,
				))
			})
			.collect()
	}

	fn backwards<E: From<Self::BackwardsError>>(x: BTreeMap<A, B>) -> Result<Self::From, E> {
		x.into_iter()
			.map(|(a, b)| {
				Ok((
					M1::backwards::<M1::BackwardsError>(a)
						.map_err(MapMigrationBackwardsError::First)
						.map_err(E::from)?,
					M2::backwards::<M2::BackwardsError>(b)
						.map_err(MapMigrationBackwardsError::Second)
						.map_err(E::from)?,
				))
			})
			.collect()
	}
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
