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

use crate::migrations::HasChangelog;

pub trait Version: Copy {
	const CANONICAL_RUNTIME_PATCH_VERSION_FOR_COMPATIBILITY_TEST: Option<CanonicalPatchVersion>;
}

pub enum CanonicalPatchVersion {
	Unreleased,
	Released(u32),
}

#[derive(Debug)]
pub enum MigrationError {
	EnumVariantDoesntExist,
}

pub trait Migration<To, V: Version> {
	type From: IsHistoricalType;
	fn forwards(x: Self::From) -> Result<To, MigrationError>;
	fn backwards(x: To) -> Result<Self::From, MigrationError>;
}

pub trait HasVersion<V: Version>: Sized {
	type HistoricalType;
	type HistoricalMigration: Migration<Self::HistoricalType, V>;
	type MigrationToCurrent: Migration<Self, vCurrent, From = Self::HistoricalType>;
}

pub fn migrate_from_historical_type<V: Version, X: HasVersion<V>>(
	_v: V,
	x: X::HistoricalType,
) -> Result<X, MigrationError> {
	X::MigrationToCurrent::forwards(x)
}

pub fn migrate_to_historical_type<V: Version, X: HasVersion<V>>(
	_v: V,
	x: X,
) -> Result<X::HistoricalType, MigrationError> {
	X::MigrationToCurrent::backwards(x)
}
// -------- identity migration --------
pub struct IdentityMigration;

impl<X: IsHistoricalType, V: Version> Migration<X, V> for IdentityMigration {
	type From = X;

	fn forwards(x: Self::From) -> Result<X, MigrationError> {
		Ok(x)
	}

	fn backwards(x: X) -> Result<Self::From, MigrationError> {
		Ok(x)
	}
}

// -------- composition of migrations --------
impl<V: Version, W: Version, X, A: Migration<B::From, W>, B: Migration<X, V>> Migration<X, V>
	for (A, W, B)
{
	type From = A::From;

	fn forwards(x: Self::From) -> Result<X, MigrationError> {
		B::forwards(A::forwards(x)?)
	}

	fn backwards(x: X) -> Result<Self::From, MigrationError> {
		A::backwards(B::backwards(x)?)
	}
}

// ------- migration for new field with default value --------

pub struct NewFieldWithDefault;
impl<T: Default, V: Version> Migration<T, V> for NewFieldWithDefault {
	type From = ();

	fn forwards(_x: Self::From) -> Result<T, MigrationError> {
		Ok(Default::default())
	}

	fn backwards(_x: T) -> Result<Self::From, MigrationError> {
		Ok(())
	}
}

// ----------- lookups ------------

pub trait IsHistoricalType {
	type GetCurrentType: HasChangelog;
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
	type MigrationFromGeneric: Migration<Self, vCurrent, From = Self::GenericType>;
}

pub type GetGenericVariant<X: HasGenericVariant> =
	<X::MigrationFromGeneric as Migration<X, vCurrent>>::From;

pub struct GlobalMigrationFromGeneric;

pub fn migrate_from_generic_type<X: HasGenericVariant>(
	x: X::GenericType,
) -> Result<X, MigrationError> {
	X::MigrationFromGeneric::forwards(x)
}

pub fn migrate_to_generic_type<X: HasGenericVariant>(
	x: X,
) -> Result<X::GenericType, MigrationError> {
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
