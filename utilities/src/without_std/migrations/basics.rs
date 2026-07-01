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

// ---------- definition of migrations ------------

use crate::never::Never;

pub trait Version: Copy {
	const CANONICAL_RUNTIME_PATCH_VERSION_FOR_COMPATIBILITY_TEST: Option<CanonicalPatchVersion>;
}

pub enum CanonicalPatchVersion {
	Unreleased,
	Released(u32),
}

pub trait Migration<To, V: Version> {
	type From: IsHistoricalType;
	type ForwardsError = Never;
	type BackwardsError = Never;
	fn forwards(x: Self::From) -> To;
	fn backwards(x: To) -> Self::From;
	fn try_forwards<E>(
		_x: Self::From,
		_map_error: impl Fn(Self::ForwardsError) -> E,
	) -> Result<To, E> {
		todo!()
	}
	fn try_backwards<E>(
		_x: To,
		_map_error: impl Fn(Self::BackwardsError) -> E,
	) -> Result<Self::From, E> {
		todo!()
	}
}

pub trait HasVersion<V: Version>: Sized {
	type HistoricalType;
	type HistoricalMigration: Migration<Self::HistoricalType, V>;
	type MigrationToCurrent: Migration<Self, vCurrent, From = Self::HistoricalType>;
}

pub fn migrate_from_historical_type<V: Version, X: HasVersion<V>>(
	_v: V,
	x: X::HistoricalType,
) -> X {
	X::MigrationToCurrent::forwards(x)
}

pub fn migrate_to_historical_type<V: Version, X: HasVersion<V>>(_v: V, x: X) -> X::HistoricalType {
	X::MigrationToCurrent::backwards(x)
}
// -------- identity migration --------
pub struct IdentityMigration;

impl<X: IsHistoricalType, V: Version> Migration<X, V> for IdentityMigration {
	type From = X;

	fn forwards(x: Self::From) -> X {
		x
	}

	fn backwards(x: X) -> Self::From {
		x
	}

	fn try_forwards<E>(
		x: Self::From,
		_map_error: impl Fn(Self::ForwardsError) -> E,
	) -> Result<X, E> {
		Ok(x)
	}

	fn try_backwards<E>(
		x: X,
		_map_error: impl Fn(Self::BackwardsError) -> E,
	) -> Result<Self::From, E> {
		Ok(x)
	}
}

// -------- composition of migrations --------
pub enum ComposedMigrationFailedForwards<A, B> {
	First(A),
	Second(B),
}

pub enum ComposedMigrationFailedBackwards<A, B> {
	First(A),
	Second(B),
}

impl<V: Version, W: Version, X, A: Migration<B::From, W>, B: Migration<X, V>> Migration<X, V>
	for (A, W, B)
{
	type From = A::From;
	type ForwardsError = ComposedMigrationFailedForwards<A::ForwardsError, B::ForwardsError>;
	type BackwardsError = ComposedMigrationFailedBackwards<A::BackwardsError, B::BackwardsError>;

	fn forwards(x: Self::From) -> X {
		B::forwards(A::forwards(x))
	}

	fn backwards(x: X) -> Self::From {
		A::backwards(B::backwards(x))
	}

	fn try_forwards<E>(
		x: Self::From,
		map_error: impl Fn(Self::ForwardsError) -> E,
	) -> Result<X, E> {
		let x =
			A::try_forwards(x, |error| map_error(ComposedMigrationFailedForwards::First(error)))?;

		B::try_forwards(x, |error| map_error(ComposedMigrationFailedForwards::Second(error)))
	}

	fn try_backwards<E>(
		x: X,
		map_error: impl Fn(Self::BackwardsError) -> E,
	) -> Result<Self::From, E> {
		let x = B::try_backwards(x, |error| {
			map_error(ComposedMigrationFailedBackwards::Second(error))
		})?;

		A::try_backwards(x, |error| map_error(ComposedMigrationFailedBackwards::First(error)))
	}
}

// ------- migration for new field with default value --------

pub struct NewFieldWithDefault;
impl<T: Default, V: Version> Migration<T, V> for NewFieldWithDefault {
	type From = ();

	fn forwards(_x: Self::From) -> T {
		Default::default()
	}

	fn backwards(_x: T) -> Self::From {}

	fn try_forwards<E>(
		_x: Self::From,
		_map_error: impl Fn(Self::ForwardsError) -> E,
	) -> Result<T, E> {
		Ok(Default::default())
	}

	fn try_backwards<E>(
		_x: T,
		_map_error: impl Fn(Self::BackwardsError) -> E,
	) -> Result<Self::From, E> {
		Ok(())
	}
}

// ------- migration for new enum variant --------

pub struct NewVariant;

#[derive(Debug)]
pub struct NewVariantBackwardsError;

impl<T, V: Version> Migration<T, V> for NewVariant {
	type From = Never;
	type BackwardsError = NewVariantBackwardsError;

	fn forwards(x: Self::From) -> T {
		match x {}
	}

	fn backwards(_x: T) -> Self::From {
		panic!("cannot migrate newly added enum variant backwards")
	}

	fn try_forwards<E>(
		x: Self::From,
		_map_error: impl Fn(Self::ForwardsError) -> E,
	) -> Result<T, E> {
		match x {}
	}

	fn try_backwards<E>(
		_x: T,
		map_error: impl Fn(Self::BackwardsError) -> E,
	) -> Result<Self::From, E> {
		Err(map_error(NewVariantBackwardsError))
	}
}

// ----------- lookups ------------

pub trait IsHistoricalType {
	type GetCurrentType;
}
pub trait IsHistoricalTypeAt<V: Version> =
	IsHistoricalType<GetCurrentType: HasVersion<V, HistoricalType = Self>>;
pub type GetMigrationToHistoricalType<X: IsHistoricalTypeAt<V>, V: Version> =
	<X::GetCurrentType as HasVersion<V>>::HistoricalMigration;

// ----------- associated generic type --------------

/// Version name for the current version of a type. Only used as the version specifier for
/// migrations between the actual type and its "generic" version.
#[derive(Clone, Copy)]
#[expect(nonstandard_style)]
pub struct vCurrent;
impl Version for vCurrent {
	// There's no released runtime version associated.
	const CANONICAL_RUNTIME_PATCH_VERSION_FOR_COMPATIBILITY_TEST: Option<CanonicalPatchVersion> =
		None;
}

pub trait HasGenericVariant: Sized {
	type GenericType;
	type MigrationFromGeneric: Migration<
		Self,
		vCurrent,
		From = Self::GenericType,
		ForwardsError = Never,
		BackwardsError = Never,
	>;
}

pub type GetGenericVariant<X: HasGenericVariant> =
	<X::MigrationFromGeneric as Migration<X, vCurrent>>::From;

pub struct GlobalMigrationFromGeneric;

pub fn migrate_from_generic_type<X: HasGenericVariant>(x: X::GenericType) -> X {
	X::MigrationFromGeneric::forwards(x)
}

pub fn migrate_to_generic_type<X: HasGenericVariant>(x: X) -> X::GenericType {
	X::MigrationFromGeneric::backwards(x)
}

// ----------- maybe migrations (for horizontal composition) ---------

pub trait MaybeMigration<To, V: Version> {
	type GetWithDefault<Default: Migration<To, V>>: Migration<To, V>;
}

pub struct DefaultMigration;
impl<To, V: Version> MaybeMigration<To, V> for DefaultMigration {
	type GetWithDefault<Default: Migration<To, V>> = Default;
}

pub struct OverrideMigrationWith<M>(M);
impl<To, V: Version, M: Migration<To, V>> MaybeMigration<To, V> for OverrideMigrationWith<M> {
	type GetWithDefault<Default: Migration<To, V>> = M;
}

impl<To, V: Version, M1: MaybeMigration<To, V>, M2: MaybeMigration<To, V>> MaybeMigration<To, V>
	for (M1, M2)
{
	type GetWithDefault<Default: Migration<To, V>> =
		M2::GetWithDefault<M1::GetWithDefault<Default>>;
}
