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

use crate::never::{IsEmptyType, Never};

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
	fn try_forwards(_x: Self::From) -> Result<To, Self::ForwardsError>;
	fn try_backwards(_x: To) -> Result<Self::From, Self::BackwardsError>;
}

pub trait HasVersion<V: Version>: Sized {
	type HistoricalType;
	type HistoricalMigration: Migration<Self::HistoricalType, V>;
	type MigrationToCurrent: Migration<Self, vCurrent, From = Self::HistoricalType>;
}

pub fn try_migrate_from_historical_type<V: Version, X: HasVersion<V>>(
	_v: V,
	x: X::HistoricalType,
) -> Result<X, <X::MigrationToCurrent as Migration<X, vCurrent>>::ForwardsError> {
	X::MigrationToCurrent::try_forwards(x)
}

pub fn try_migrate_to_historical_type<V: Version, X: HasVersion<V>>(
	_v: V,
	x: X,
) -> Result<X::HistoricalType, <X::MigrationToCurrent as Migration<X, vCurrent>>::BackwardsError> {
	X::MigrationToCurrent::try_backwards(x)
}

pub fn migrate_from_historical_type<V: Version, X: HasVersion<V>>(_v: V, x: X::HistoricalType) -> X
where
	<X::MigrationToCurrent as Migration<X, vCurrent>>::ForwardsError: IsEmptyType,
{
	match X::MigrationToCurrent::try_forwards(x) {
		Ok(x) => x,
		#[allow(unreachable_code)]
		Err(empty) => match empty.as_never() {},
	}
}

pub fn migrate_to_historical_type<V: Version, X: HasVersion<V>>(_v: V, x: X) -> X::HistoricalType
where
	<X::MigrationToCurrent as Migration<X, vCurrent>>::BackwardsError: IsEmptyType,
{
	match X::MigrationToCurrent::try_backwards(x) {
		Ok(x) => x,
		#[allow(unreachable_code)]
		Err(empty) => match empty.as_never() {},
	}
}
// -------- identity migration --------
pub struct IdentityMigration;

impl<X: IsHistoricalType, V: Version> Migration<X, V> for IdentityMigration {
	type From = X;

	fn try_forwards(x: Self::From) -> Result<X, Self::ForwardsError> {
		Ok(x)
	}

	fn try_backwards(x: X) -> Result<Self::From, Self::BackwardsError> {
		Ok(x)
	}
}

// -------- composition of migrations --------

pub enum ComposedMigrationFailed<A, B> {
	First(A),
	Second(B),
}

impl<A: IsEmptyType, B: IsEmptyType> IsEmptyType for ComposedMigrationFailed<A, B> {
	fn as_never(&self) -> Never {
		match self {
			Self::First(error) => error.as_never(),
			Self::Second(error) => error.as_never(),
		}
	}
}

impl<V: Version, W: Version, X, A: Migration<B::From, W>, B: Migration<X, V>> Migration<X, V>
	for (A, W, B)
{
	type From = A::From;
	type ForwardsError = ComposedMigrationFailed<A::ForwardsError, B::ForwardsError>;
	type BackwardsError = ComposedMigrationFailed<A::BackwardsError, B::BackwardsError>;

	fn try_forwards(x: Self::From) -> Result<X, Self::ForwardsError> {
		let x = A::try_forwards(x).map_err(ComposedMigrationFailed::First)?;
		B::try_forwards(x).map_err(ComposedMigrationFailed::Second)
	}

	fn try_backwards(x: X) -> Result<Self::From, Self::BackwardsError> {
		let x = B::try_backwards(x).map_err(ComposedMigrationFailed::Second)?;
		A::try_backwards(x).map_err(ComposedMigrationFailed::First)
	}
}

// ------- migration for new field with default value --------

pub struct NewFieldWithDefault;
impl<T: Default, V: Version> Migration<T, V> for NewFieldWithDefault {
	type From = ();

	fn try_forwards(_x: Self::From) -> Result<T, Self::ForwardsError> {
		Ok(Default::default())
	}

	fn try_backwards(_x: T) -> Result<Self::From, Self::BackwardsError> {
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

	fn try_forwards(x: Self::From) -> Result<T, Self::ForwardsError> {
		match x {}
	}

	fn try_backwards(_x: T) -> Result<Self::From, Self::BackwardsError> {
		Err(NewVariantBackwardsError)
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

pub fn try_migrate_from_generic_type<X: HasGenericVariant>(x: X::GenericType) -> X {
	match X::MigrationFromGeneric::try_forwards(x) {
		Ok(x) => x,
		#[allow(unreachable_code)]
		Err(err) => match err.as_never() {},
	}
}

pub fn try_migrate_to_generic_type<X: HasGenericVariant>(x: X) -> X::GenericType {
	match X::MigrationFromGeneric::try_backwards(x) {
		Ok(x) => x,
		#[allow(unreachable_code)]
		Err(err) => match err.as_never() {},
	}
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
