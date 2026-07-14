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
		basics::{vCurrent, IdentityMigration, Migration, Version},
		with_all_runtime_migrations, HasChangelog, HasGenericVariant, IsHistoricalType,
		OrdMigrations,
	},
	never::{IsEmptyType, Never},
	type_introspection::HasTypeIntrospection,
};

// ----------- identity migrations -------------

#[macro_export]
macro_rules! impl_identity_migrations {
	($($ty:ty, )*) => {

        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl $crate::migrations::basics::IsHistoricalType for Type {
            type GetCurrentType = Self;
        }
        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl $crate::migrations::basics::HasGenericVariant for Type {
            type GenericType = Type;
            type MigrationFromGeneric = $crate::migrations::basics::IdentityMigration;
        }
        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl $crate::migrations::HasChangelog for Type {
            type if_unspecified = $crate::migrations::basics::IdentityMigration;
        }
    };
}
pub use impl_identity_migrations;

impl_identity_migrations! {(), bool, u8, u16, u32, u64, u128, Never, [u8; 20], [u8; 32], }

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

			fn try_forwards(x: Self::From) -> Result<$Ty, Self::ForwardsError> {
				Ok(x.0)
			}

			fn try_backwards(x: $Ty) -> Result<Self::From, Self::BackwardsError> {
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

        $(
            // This implementation assumes the inner type has a Default implementation
            impl HasTypeIntrospection for $Wrapper
                where $Inner: Default
            {
                fn is_empty_type() -> bool {
                    false
                }

                fn sample_all_shapes() -> Vec<Self> {
                    let $var = <$Inner as Default>::default();
                    sp_std::vec![$Wrapper($ctr)]
                }
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

impl_identity_migrations_with_wrapper! {
	#[derive(PartialOrd, PartialEq, Eq, Ord, Default)]
	struct WrappedU256(sp_core::U256) where |x: [u64; 4]| sp_core::U256(x);
}

// ----------- simple migration that introduces a new type -------------

#[derive(
	codec::Encode,
	codec::Decode,
	scale_info::TypeInfo,
	PartialEq,
	Debug,
	cf_proc_macros::HasTypeIntrospection,
)]
#[cfg_attr(all(feature = "proptest", feature = "std"), derive(proptest_derive::Arbitrary))]
pub struct HistoricalEmptyPlaceholder<T>(sp_std::marker::PhantomData<T>);
impl<T: HasGenericVariant + HasChangelog> IsHistoricalType for HistoricalEmptyPlaceholder<T> {
	type GetCurrentType = T;
}

pub struct NewTypeWithDefault;
impl<V: Version, T: HasChangelog + Default> Migration<T, V> for NewTypeWithDefault {
	type From = HistoricalEmptyPlaceholder<T>;

	fn try_forwards(_: Self::From) -> Result<T, Self::ForwardsError> {
		Ok(Default::default())
	}

	fn try_backwards(_: T) -> Result<Self::From, Self::BackwardsError> {
		Ok(HistoricalEmptyPlaceholder(Default::default()))
	}
}

// ----------- containers -------------

pub struct MapMigration<X>(X);

pub struct GenericMapMigration<X>(X);

pub enum OptionMigrationFailed<E> {
	Some(E),
}

pub enum VecMigrationFailed<E> {
	Element { index: usize, error: E },
}

pub enum TupleWith1EntryMigrationFailed<E> {
	First(E),
}

pub enum TupleWith2EntriesMigrationFailed<A, B> {
	First(A),
	Second(B),
}

impl<E: IsEmptyType> IsEmptyType for OptionMigrationFailed<E> {
	fn as_never(&self) -> Never {
		match self {
			Self::Some(error) => error.as_never(),
		}
	}
}

impl<E: IsEmptyType> IsEmptyType for VecMigrationFailed<E> {
	fn as_never(&self) -> Never {
		match self {
			Self::Element { error, .. } => error.as_never(),
		}
	}
}

impl<E: IsEmptyType> IsEmptyType for TupleWith1EntryMigrationFailed<E> {
	fn as_never(&self) -> Never {
		match self {
			Self::First(error) => error.as_never(),
		}
	}
}

impl<A: IsEmptyType, B: IsEmptyType> IsEmptyType for TupleWith2EntriesMigrationFailed<A, B> {
	fn as_never(&self) -> Never {
		match self {
			Self::First(error) => error.as_never(),
			Self::Second(error) => error.as_never(),
		}
	}
}

macro_rules! impl_migrations_for_container {
    (
        $container:ident<$($ty:ident $(: ($($ty_path:tt)*))?),+>,
        $container_macro:ident,
        [$($var_M:ident $(where From: ($($from_path:tt)*) )?),+],
		type ForwardsError = $forwards_error:ty,
		type BackwardsError = $backwards_error:ty,
		try_forwards |$var_try_f:ident| $expr_try_f:expr,
		try_backwards |$var_try_b:ident| $expr_try_b:expr,
		generic_try_forwards |$var_generic_try_f:ident| $expr_generic_try_f:expr,
		generic_try_backwards |$var_generic_try_b:ident| $expr_generic_try_b:expr,
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

        impl<$($ty $(: $($ty_path)* )? ,)+  V: Version, $($var_M: Migration<$ty, V $(, From: $($from_path)*)?>),+> Migration<$container<$($ty),+>, V> for MapMigration<($($var_M, )+)> {
            type From = $container<$($var_M::From),+>;
			type ForwardsError = $forwards_error;
			type BackwardsError = $backwards_error;

			fn try_forwards($var_try_f: Self::From) -> Result<$container<$($ty),+>, Self::ForwardsError> {
				$expr_try_f
			}

			fn try_backwards($var_try_b: $container<$($ty),+>) -> Result<Self::From, Self::BackwardsError> {
				$expr_try_b
			}
        }

		impl<$($ty $(: $($ty_path)* )? ,)+ $($var_M: Migration<$ty, vCurrent, ForwardsError = Never, BackwardsError = Never $(, From: $($from_path)*)?>),+> Migration<$container<$($ty),+>, vCurrent> for GenericMapMigration<($($var_M, )+)> {
			type From = $container<$($var_M::From),+>;

			fn try_forwards($var_generic_try_f: Self::From) -> Result<$container<$($ty),+>, Self::ForwardsError> {
				$expr_generic_try_f
			}

			fn try_backwards($var_generic_try_b: $container<$($ty),+>) -> Result<Self::From, Self::BackwardsError> {
				$expr_generic_try_b
			}
		}

        impl<$($ty: HasGenericVariant $(+ $($ty_path)*)?),+ > HasGenericVariant for $container<$($ty),+> {
            type GenericType = $container<$($ty::GenericType),+>;
            type MigrationFromGeneric = GenericMapMigration<($($ty::MigrationFromGeneric, )+)>;
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
	type ForwardsError = OptionMigrationFailed<M::ForwardsError>,
	type BackwardsError = OptionMigrationFailed<M::BackwardsError>,
	try_forwards |x| {
		match x {
			Some(x) => M::try_forwards(x).map_err(OptionMigrationFailed::Some).map(Some),
			None => Ok(None),
		}
	},
	try_backwards |x| {
		match x {
			Some(x) => M::try_backwards(x).map_err(OptionMigrationFailed::Some).map(Some),
			None => Ok(None),
		}
	},
	generic_try_forwards |x| {
		match x {
			Some(x) => match M::try_forwards(x) {
				Ok(x) => Ok(Some(x)),
				Err(error) => match error {},
			},
			None => Ok(None),
		}
	},
	generic_try_backwards |x| {
		match x {
			Some(x) => match M::try_backwards(x) {
				Ok(x) => Ok(Some(x)),
				Err(error) => match error {},
			},
			None => Ok(None),
		}
	},
}

impl_migrations_for_container! {
	Vec<X>,
	impl_changelog_for_vector,
	[M],
	type ForwardsError = VecMigrationFailed<M::ForwardsError>,
	type BackwardsError = VecMigrationFailed<M::BackwardsError>,
	try_forwards |x| {
		let mut result = Vec::with_capacity(x.len());

		for (index, x) in x.into_iter().enumerate() {
			result.push(
				M::try_forwards(x).map_err(|error| VecMigrationFailed::Element { index, error })?,
			);
		}

		Ok(result)
	},
	try_backwards |x| {
		let mut result = Vec::with_capacity(x.len());

		for (index, x) in x.into_iter().enumerate() {
			result.push(
				M::try_backwards(x).map_err(|error| VecMigrationFailed::Element { index, error })?,
			);
		}

		Ok(result)
	},
	generic_try_forwards |x| {
		let mut result = Vec::with_capacity(x.len());

		for x in x {
			result.push(M::try_forwards(x)?);
		}

		Ok(result)
	},
	generic_try_backwards |x| {
		let mut result = Vec::with_capacity(x.len());

		for x in x {
			result.push(M::try_backwards(x)?);
		}

		Ok(result)
	},
}

pub type TupleWith1Entry<A> = (A,);

impl_migrations_for_container! {
	TupleWith1Entry<A>,
	impl_changelog_for_tuple1,
	[M1],
	type ForwardsError = TupleWith1EntryMigrationFailed<M1::ForwardsError>,
	type BackwardsError = TupleWith1EntryMigrationFailed<M1::BackwardsError>,
	try_forwards |x| {
		Ok((M1::try_forwards(x.0).map_err(TupleWith1EntryMigrationFailed::First)?,))
	},
	try_backwards |x| {
		Ok((M1::try_backwards(x.0).map_err(TupleWith1EntryMigrationFailed::First)?,))
	},
	generic_try_forwards |x| {
		Ok((M1::try_forwards(x.0)?,))
	},
	generic_try_backwards |x| {
		Ok((M1::try_backwards(x.0)?,))
	},
}

pub type TupleWith2Entries<A, B> = (A, B);

impl_migrations_for_container! {
	TupleWith2Entries<A,B>,
	impl_changelog_for_tuple,
	[M1,M2],
	type ForwardsError = TupleWith2EntriesMigrationFailed<M1::ForwardsError, M2::ForwardsError>,
	type BackwardsError = TupleWith2EntriesMigrationFailed<M1::BackwardsError, M2::BackwardsError>,
	try_forwards |x| {
		Ok((
			M1::try_forwards(x.0).map_err(TupleWith2EntriesMigrationFailed::First)?,
			M2::try_forwards(x.1).map_err(TupleWith2EntriesMigrationFailed::Second)?,
		))
	},
	try_backwards |x| {
		Ok((
			M1::try_backwards(x.0).map_err(TupleWith2EntriesMigrationFailed::First)?,
			M2::try_backwards(x.1).map_err(TupleWith2EntriesMigrationFailed::Second)?,
		))
	},
	generic_try_forwards |x| {
		Ok((M1::try_forwards(x.0)?, M2::try_forwards(x.1)?))
	},
	generic_try_backwards |x| {
		Ok((M1::try_backwards(x.0)?, M2::try_backwards(x.1)?))
	},
}

// ---- btreemap ----
// the bounds are quite messy and difficult to replicate with the `impl_migrations_for_container`
// macro, so we use a manual implementation:

pub enum BTreeMapMigrationFailed<KeyError, ValueError> {
	Key(KeyError),
	Value(ValueError),
	KeyCollision,
}

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
	type ForwardsError = BTreeMapMigrationFailed<M1::ForwardsError, M2::ForwardsError>;
	type BackwardsError = BTreeMapMigrationFailed<M1::BackwardsError, M2::BackwardsError>;

	fn try_forwards(x: Self::From) -> Result<BTreeMap<A, B>, Self::ForwardsError> {
		let mut result = BTreeMap::new();

		for (a, b) in x {
			let a = M1::try_forwards(a).map_err(BTreeMapMigrationFailed::Key)?;
			let b = M2::try_forwards(b).map_err(BTreeMapMigrationFailed::Value)?;

			if result.insert(a, b).is_some() {
				return Err(BTreeMapMigrationFailed::KeyCollision);
			}
		}

		Ok(result)
	}

	fn try_backwards(x: BTreeMap<A, B>) -> Result<Self::From, Self::BackwardsError> {
		let mut result = BTreeMap::new();

		for (a, b) in x {
			let a = M1::try_backwards(a).map_err(BTreeMapMigrationFailed::Key)?;
			let b = M2::try_backwards(b).map_err(BTreeMapMigrationFailed::Value)?;

			if result.insert(a, b).is_some() {
				return Err(BTreeMapMigrationFailed::KeyCollision);
			}
		}

		Ok(result)
	}
}

impl<
		A: Ord,
		B,
		M1: Migration<
			A,
			vCurrent,
			From: IsHistoricalType<GetCurrentType: OrdMigrations + Ord> + Ord,
			ForwardsError = Never,
			BackwardsError = Never,
		>,
		M2: Migration<B, vCurrent, ForwardsError = Never, BackwardsError = Never>,
	> Migration<BTreeMap<A, B>, vCurrent> for GenericMapMigration<(M1, M2)>
{
	type From = BTreeMap<M1::From, M2::From>;

	fn try_forwards(x: Self::From) -> Result<BTreeMap<A, B>, Self::ForwardsError> {
		let mut result = BTreeMap::new();

		for (a, b) in x {
			let a = M1::try_forwards(a)?;
			let b = M2::try_forwards(b)?;
			result.insert(a, b);
		}

		Ok(result)
	}

	fn try_backwards(x: BTreeMap<A, B>) -> Result<Self::From, Self::BackwardsError> {
		let mut result = BTreeMap::new();

		for (a, b) in x {
			let a = M1::try_backwards(a)?;
			let b = M2::try_backwards(b)?;
			result.insert(a, b);
		}

		Ok(result)
	}
}

impl<A: HasGenericVariant + Ord, B: HasGenericVariant> HasGenericVariant for BTreeMap<A, B>
where
	A: HasGenericVariant<GenericType: Ord + IsHistoricalTypeOrd>,
{
	type GenericType = BTreeMap<A::GenericType, B::GenericType>;
	type MigrationFromGeneric =
		GenericMapMigration<(A::MigrationFromGeneric, B::MigrationFromGeneric)>;
}
impl<A: IsHistoricalType<GetCurrentType: OrdMigrations + Ord>, B: IsHistoricalType> IsHistoricalType
	for BTreeMap<A, B>
{
	type GetCurrentType = BTreeMap<A::GetCurrentType, B::GetCurrentType>;
}

trait IsHistoricalTypeOrd = IsHistoricalType<GetCurrentType: OrdMigrations + Ord>;
