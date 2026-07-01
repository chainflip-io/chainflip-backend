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

impl_identity_migrations! {(), bool, u16, u32, u64, u128, u8, Never, }

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

			fn forwards(x: Self::From) -> $Ty {
				x.0
			}

			fn backwards(x: $Ty) -> Self::From {
				$Wrapper(x)
			}

			fn try_forwards<E>(
				x: Self::From,
				_map_error: impl Fn(Self::ForwardsError) -> E,
			) -> Result<$Ty, E> {
				Ok(x.0)
			}

			fn try_backwards<E>(
				x: $Ty,
				_map_error: impl Fn(Self::BackwardsError) -> E,
			) -> Result<Self::From, E> {
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

	fn forwards(_: Self::From) -> T {
		Default::default()
	}

	fn backwards(_: T) -> Self::From {
		HistoricalEmptyPlaceholder(Default::default())
	}

	fn try_forwards<E>(
		_: Self::From,
		_map_error: impl Fn(Self::ForwardsError) -> E,
	) -> Result<T, E> {
		Ok(Default::default())
	}

	fn try_backwards<E>(
		_: T,
		_map_error: impl Fn(Self::BackwardsError) -> E,
	) -> Result<Self::From, E> {
		Ok(HistoricalEmptyPlaceholder(Default::default()))
	}
}

// ----------- containers -------------

pub struct MapMigration<X>(X);

pub struct GenericMapMigration<X>(X);

pub enum OptionMigrationFailedForwards<E> {
	Some(E),
}

pub enum OptionMigrationFailedBackwards<E> {
	Some(E),
}

pub enum VecMigrationFailedForwards<E> {
	Element { index: usize, error: E },
}

pub enum VecMigrationFailedBackwards<E> {
	Element { index: usize, error: E },
}

pub enum TupleWith1EntryMigrationFailedForwards<E> {
	First(E),
}

pub enum TupleWith1EntryMigrationFailedBackwards<E> {
	First(E),
}

pub enum TupleWith2EntriesMigrationFailedForwards<A, B> {
	First(A),
	Second(B),
}

pub enum TupleWith2EntriesMigrationFailedBackwards<A, B> {
	First(A),
	Second(B),
}

impl<E: IsEmptyType> IsEmptyType for OptionMigrationFailedForwards<E> {
	fn as_never(self) -> Never {
		match self {
			Self::Some(error) => error.as_never(),
		}
	}
}

impl<E: IsEmptyType> IsEmptyType for OptionMigrationFailedBackwards<E> {
	fn as_never(self) -> Never {
		match self {
			Self::Some(error) => error.as_never(),
		}
	}
}

impl<E: IsEmptyType> IsEmptyType for VecMigrationFailedForwards<E> {
	fn as_never(self) -> Never {
		match self {
			Self::Element { error, .. } => error.as_never(),
		}
	}
}

impl<E: IsEmptyType> IsEmptyType for VecMigrationFailedBackwards<E> {
	fn as_never(self) -> Never {
		match self {
			Self::Element { error, .. } => error.as_never(),
		}
	}
}

impl<E: IsEmptyType> IsEmptyType for TupleWith1EntryMigrationFailedForwards<E> {
	fn as_never(self) -> Never {
		match self {
			Self::First(error) => error.as_never(),
		}
	}
}

impl<E: IsEmptyType> IsEmptyType for TupleWith1EntryMigrationFailedBackwards<E> {
	fn as_never(self) -> Never {
		match self {
			Self::First(error) => error.as_never(),
		}
	}
}

impl<A: IsEmptyType, B: IsEmptyType> IsEmptyType
	for TupleWith2EntriesMigrationFailedForwards<A, B>
{
	fn as_never(self) -> Never {
		match self {
			Self::First(error) => error.as_never(),
			Self::Second(error) => error.as_never(),
		}
	}
}

impl<A: IsEmptyType, B: IsEmptyType> IsEmptyType
	for TupleWith2EntriesMigrationFailedBackwards<A, B>
{
	fn as_never(self) -> Never {
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
        |$var_f:ident| $expr_f:expr,
        |$var_b:ident| $expr_b:expr,
		type ForwardsError = $forwards_error:ty,
		type BackwardsError = $backwards_error:ty,
		try_forwards |$var_try_f:ident, $map_error_f:ident| $expr_try_f:expr,
		try_backwards |$var_try_b:ident, $map_error_b:ident| $expr_try_b:expr,
		never_forwards |$never_error_f:ident| $expr_never_f:expr,
		never_backwards |$never_error_b:ident| $expr_never_b:expr,
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

            fn forwards($var_f: Self::From) -> $container<$($ty),+> {
                $expr_f
            }

            fn backwards($var_b: $container<$($ty),+>) -> Self::From {
                $expr_b
            }

			fn try_forwards<E>(
				$var_try_f: Self::From,
				$map_error_f: impl Fn(Self::ForwardsError) -> E,
			) -> Result<$container<$($ty),+>, E> {
				$expr_try_f
			}

			fn try_backwards<E>(
				$var_try_b: $container<$($ty),+>,
				$map_error_b: impl Fn(Self::BackwardsError) -> E,
			) -> Result<Self::From, E> {
				$expr_try_b
			}
        }

		impl<$($ty $(: $($ty_path)* )? ,)+ $($var_M: Migration<$ty, vCurrent, ForwardsError = Never, BackwardsError = Never $(, From: $($from_path)*)?>),+> Migration<$container<$($ty),+>, vCurrent> for GenericMapMigration<($($var_M, )+)> {
			type From = $container<$($var_M::From),+>;

			fn forwards($var_f: Self::From) -> $container<$($ty),+> {
				$expr_f
			}

			fn backwards($var_b: $container<$($ty),+>) -> Self::From {
				$expr_b
			}

			fn try_forwards<E>(
				$var_try_f: Self::From,
				_map_error: impl Fn(Self::ForwardsError) -> E,
			) -> Result<$container<$($ty),+>, E> {
				let $map_error_f = |$never_error_f| -> E { $expr_never_f };

				$expr_try_f
			}

			fn try_backwards<E>(
				$var_try_b: $container<$($ty),+>,
				_map_error: impl Fn(Self::BackwardsError) -> E,
			) -> Result<Self::From, E> {
				let $map_error_b = |$never_error_b| -> E { $expr_never_b };

				$expr_try_b
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
	|x| x.map(M::forwards),
	|x| x.map(M::backwards),
	type ForwardsError = OptionMigrationFailedForwards<M::ForwardsError>,
	type BackwardsError = OptionMigrationFailedBackwards<M::BackwardsError>,
	try_forwards |x, map_error| {
		match x {
			Some(x) => M::try_forwards(x, |error| {
				map_error(OptionMigrationFailedForwards::Some(error))
			})
			.map(Some),
			None => Ok(None),
		}
	},
	try_backwards |x, map_error| {
		match x {
			Some(x) => M::try_backwards(x, |error| {
				map_error(OptionMigrationFailedBackwards::Some(error))
			})
			.map(Some),
			None => Ok(None),
		}
	},
	never_forwards |error| match error {
		OptionMigrationFailedForwards::Some(error) => match error {},
	},
	never_backwards |error| match error {
		OptionMigrationFailedBackwards::Some(error) => match error {},
	},
}

impl_migrations_for_container! {
	Vec<X>,
	impl_changelog_for_vector,
	[M],
	|x| x.into_iter().map(M::forwards).collect(),
	|x| x.into_iter().map(M::backwards).collect(),
	type ForwardsError = VecMigrationFailedForwards<M::ForwardsError>,
	type BackwardsError = VecMigrationFailedBackwards<M::BackwardsError>,
	try_forwards |x, map_error| {
		let mut result = Vec::with_capacity(x.len());

		for (index, x) in x.into_iter().enumerate() {
			result.push(M::try_forwards(x, |error| {
				map_error(VecMigrationFailedForwards::Element { index, error })
			})?);
		}

		Ok(result)
	},
	try_backwards |x, map_error| {
		let mut result = Vec::with_capacity(x.len());

		for (index, x) in x.into_iter().enumerate() {
			result.push(M::try_backwards(x, |error| {
				map_error(VecMigrationFailedBackwards::Element { index, error })
			})?);
		}

		Ok(result)
	},
	never_forwards |error| match error {
		VecMigrationFailedForwards::Element { error, .. } => match error {},
	},
	never_backwards |error| match error {
		VecMigrationFailedBackwards::Element { error, .. } => match error {},
	},
}

pub type TupleWith1Entry<A> = (A,);

impl_migrations_for_container! {
	TupleWith1Entry<A>,
	impl_changelog_for_tuple1,
	[M1],
	|x| (M1::forwards(x.0),),
	|x| (M1::backwards(x.0),),
	type ForwardsError = TupleWith1EntryMigrationFailedForwards<M1::ForwardsError>,
	type BackwardsError = TupleWith1EntryMigrationFailedBackwards<M1::BackwardsError>,
	try_forwards |x, map_error| {
		Ok((M1::try_forwards(x.0, |error| {
			map_error(TupleWith1EntryMigrationFailedForwards::First(error))
		})?,))
	},
	try_backwards |x, map_error| {
		Ok((M1::try_backwards(x.0, |error| {
			map_error(TupleWith1EntryMigrationFailedBackwards::First(error))
		})?,))
	},
	never_forwards |error| match error {
		TupleWith1EntryMigrationFailedForwards::First(error) => match error {},
	},
	never_backwards |error| match error {
		TupleWith1EntryMigrationFailedBackwards::First(error) => match error {},
	},
}

pub type TupleWith2Entries<A, B> = (A, B);

impl_migrations_for_container! {
	TupleWith2Entries<A,B>,
	impl_changelog_for_tuple,
	[M1,M2],
	|x| (M1::forwards(x.0), M2::forwards(x.1)),
	|x| (M1::backwards(x.0), M2::backwards(x.1)),
	type ForwardsError = TupleWith2EntriesMigrationFailedForwards<M1::ForwardsError, M2::ForwardsError>,
	type BackwardsError = TupleWith2EntriesMigrationFailedBackwards<M1::BackwardsError, M2::BackwardsError>,
	try_forwards |x, map_error| {
		Ok((
			M1::try_forwards(x.0, |error| {
				map_error(TupleWith2EntriesMigrationFailedForwards::First(error))
			})?,
			M2::try_forwards(x.1, |error| {
				map_error(TupleWith2EntriesMigrationFailedForwards::Second(error))
			})?,
		))
	},
	try_backwards |x, map_error| {
		Ok((
			M1::try_backwards(x.0, |error| {
				map_error(TupleWith2EntriesMigrationFailedBackwards::First(error))
			})?,
			M2::try_backwards(x.1, |error| {
				map_error(TupleWith2EntriesMigrationFailedBackwards::Second(error))
			})?,
		))
	},
	never_forwards |error| match error {
		TupleWith2EntriesMigrationFailedForwards::First(error) => match error {},
		TupleWith2EntriesMigrationFailedForwards::Second(error) => match error {},
	},
	never_backwards |error| match error {
		TupleWith2EntriesMigrationFailedBackwards::First(error) => match error {},
		TupleWith2EntriesMigrationFailedBackwards::Second(error) => match error {},
	},
}

// ---- btreemap ----
// the bounds are quite messy and difficult to replicate with the `impl_migrations_for_container`
// macro, so we use a manual implementation:

pub enum BTreeMapMigrationFailedForwards<KeyError, ValueError> {
	Key(KeyError),
	Value(ValueError),
	KeyCollision,
}

pub enum BTreeMapMigrationFailedBackwards<KeyError, ValueError> {
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
	type ForwardsError = BTreeMapMigrationFailedForwards<M1::ForwardsError, M2::ForwardsError>;
	type BackwardsError = BTreeMapMigrationFailedBackwards<M1::BackwardsError, M2::BackwardsError>;

	fn forwards(x: Self::From) -> BTreeMap<A, B> {
		x.into_iter().map(|(a, b)| (M1::forwards(a), M2::forwards(b))).collect()
	}

	fn backwards(x: BTreeMap<A, B>) -> Self::From {
		x.into_iter().map(|(a, b)| (M1::backwards(a), M2::backwards(b))).collect()
	}

	fn try_forwards<E>(
		x: Self::From,
		map_error: impl Fn(Self::ForwardsError) -> E,
	) -> Result<BTreeMap<A, B>, E> {
		let mut result = BTreeMap::new();

		for (a, b) in x {
			let a = M1::try_forwards(a, |error| {
				map_error(BTreeMapMigrationFailedForwards::Key(error))
			})?;
			let b = M2::try_forwards(b, |error| {
				map_error(BTreeMapMigrationFailedForwards::Value(error))
			})?;

			if result.insert(a, b).is_some() {
				return Err(map_error(BTreeMapMigrationFailedForwards::KeyCollision));
			}
		}

		Ok(result)
	}

	fn try_backwards<E>(
		x: BTreeMap<A, B>,
		map_error: impl Fn(Self::BackwardsError) -> E,
	) -> Result<Self::From, E> {
		let mut result = BTreeMap::new();

		for (a, b) in x {
			let a = M1::try_backwards(a, |error| {
				map_error(BTreeMapMigrationFailedBackwards::Key(error))
			})?;
			let b = M2::try_backwards(b, |error| {
				map_error(BTreeMapMigrationFailedBackwards::Value(error))
			})?;

			if result.insert(a, b).is_some() {
				return Err(map_error(BTreeMapMigrationFailedBackwards::KeyCollision));
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

	fn forwards(x: Self::From) -> BTreeMap<A, B> {
		x.into_iter().map(|(a, b)| (M1::forwards(a), M2::forwards(b))).collect()
	}

	fn backwards(x: BTreeMap<A, B>) -> Self::From {
		x.into_iter().map(|(a, b)| (M1::backwards(a), M2::backwards(b))).collect()
	}

	fn try_forwards<E>(
		x: Self::From,
		_map_error: impl Fn(Self::ForwardsError) -> E,
	) -> Result<BTreeMap<A, B>, E> {
		let mut result = BTreeMap::new();

		for (a, b) in x {
			let a = M1::try_forwards(a, |error| match error {})?;
			let b = M2::try_forwards(b, |error| match error {})?;
			result.insert(a, b);
		}

		Ok(result)
	}

	fn try_backwards<E>(
		x: BTreeMap<A, B>,
		_map_error: impl Fn(Self::BackwardsError) -> E,
	) -> Result<Self::From, E> {
		let mut result = BTreeMap::new();

		for (a, b) in x {
			let a = M1::try_backwards(a, |error| match error {})?;
			let b = M2::try_backwards(b, |error| match error {})?;
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
