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

use crate::{migrations::HasChangelog, never::Never};

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

pub trait Migration<To, V: Version, EF, EB> {
	type From: IsHistoricalType<EF, EB>;
	fn forwards(x: Self::From) -> Result<To, EF>;
	fn backwards(x: To) -> Result<Self::From, EB>;
}

pub trait HasVersion<V: Version, EF, EB>: Sized {
	type HistoricalType;
	type HistoricalMigration: Migration<Self::HistoricalType, V, EF, EB>;
	type MigrationToCurrent: Migration<Self, vCurrent, EF, EB, From = Self::HistoricalType>;
}

pub fn try_migrate_from_historical_type<V: Version, X: HasVersion<V, EF, EB>, EF, EB>(
	_v: V,
	x: X::HistoricalType,
) -> Result<X, EF> {
	X::MigrationToCurrent::forwards(x)
}

pub fn migrate_from_historical_type<V: Version, X: HasVersion<V, Never, EB>, EB>(
	_v: V,
	x: X::HistoricalType,
) -> X {
	match X::MigrationToCurrent::forwards(x) {
		Ok(x) => x,
		Err(never) => match never {},
	}
}

pub fn try_migrate_to_historical_type<V: Version, X: HasVersion<V, EF, EB>, EF, EB>(
	_v: V,
	x: X,
) -> Result<X::HistoricalType, EB> {
	X::MigrationToCurrent::backwards(x)
}

pub fn migrate_to_historical_type<V: Version, X: HasVersion<V, EF, Never>, EF>(
	_v: V,
	x: X,
) -> X::HistoricalType {
	match X::MigrationToCurrent::backwards(x) {
		Ok(x) => x,
		Err(never) => match never {},
	}
}
// -------- identity migration --------
pub struct IdentityMigration;

impl<X: IsHistoricalType<EF, EB>, V: Version, EF, EB> Migration<X, V, EF, EB>
	for IdentityMigration
{
	type From = X;

	fn forwards(x: Self::From) -> Result<X, EF> {
		Ok(x)
	}

	fn backwards(x: X) -> Result<Self::From, EB> {
		Ok(x)
	}
}

// -------- composition of migrations --------
impl<
		V: Version,
		W: Version,
		X,
		EF,
		EB,
		A: Migration<B::From, W, EF, EB>,
		B: Migration<X, V, EF, EB>,
	> Migration<X, V, EF, EB> for (A, W, B)
{
	type From = A::From;

	fn forwards(x: Self::From) -> Result<X, EF> {
		B::forwards(A::forwards(x)?)
	}

	fn backwards(x: X) -> Result<Self::From, EB> {
		A::backwards(B::backwards(x)?)
	}
}

// ------- migration for new field with default value --------

pub struct NewFieldWithDefault;
impl<T: Default, V: Version, EF, EB> Migration<T, V, EF, EB> for NewFieldWithDefault {
	type From = ();

	fn forwards(_x: Self::From) -> Result<T, EF> {
		Ok(Default::default())
	}

	fn backwards(_x: T) -> Result<Self::From, EB> {
		Ok(())
	}
}

// ----------- lookups ------------

pub trait IsHistoricalType<EF, EB> {
	type GetCurrentType: HasChangelog<EF, EB>;
}
pub trait IsHistoricalTypeAt<V: Version, EF, EB> =
	IsHistoricalType<EF, EB, GetCurrentType: HasVersion<V, EF, EB, HistoricalType = Self>>;
pub type GetMigrationToHistoricalType<
	X: IsHistoricalType<EF, EB, GetCurrentType: HasVersion<V, EF, EB, HistoricalType = X>>,
	V: Version,
	EF,
	EB,
> = <X::GetCurrentType as HasVersion<V, EF, EB>>::HistoricalMigration;

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

pub trait HasGenericVariant<EF, EB>: Sized {
	type GenericType;
	type MigrationFromGeneric: Migration<Self, vCurrent, EF, EB, From = Self::GenericType>;
}

pub type GetGenericVariant<X: HasGenericVariant<EF, EB>, EF, EB> = <<X as HasGenericVariant<
	EF,
	EB,
>>::MigrationFromGeneric as Migration<
	X,
	vCurrent,
	EF,
	EB,
>>::From;

pub struct GlobalMigrationFromGeneric;

pub fn migrate_from_generic_type<X: HasGenericVariant<EF, EB>, EF, EB>(
	x: X::GenericType,
) -> Result<X, EF> {
	X::MigrationFromGeneric::forwards(x)
}

pub fn migrate_to_generic_type<X: HasGenericVariant<EF, EB>, EF, EB>(
	x: X,
) -> Result<X::GenericType, EB> {
	X::MigrationFromGeneric::backwards(x)
}

// ----------- maybe migrations (for horizontal composition) ---------

pub trait MaybeMigration<To, V: Version, EF, EB> {
	type GetWithDefault<Default: Migration<To, V, EF, EB>>: Migration<To, V, EF, EB>;
}

pub struct DefaultMigration;
impl<To, V: Version, EF, EB> MaybeMigration<To, V, EF, EB> for DefaultMigration {
	type GetWithDefault<Default: Migration<To, V, EF, EB>> = Default;
}

pub struct OverrideMigrationWith<M>(M);
impl<To, V: Version, EF, EB, M: Migration<To, V, EF, EB>> MaybeMigration<To, V, EF, EB>
	for OverrideMigrationWith<M>
{
	type GetWithDefault<Default: Migration<To, V, EF, EB>> = M;
}

impl<
		To,
		V: Version,
		EF,
		EB,
		M1: MaybeMigration<To, V, EF, EB>,
		M2: MaybeMigration<To, V, EF, EB>,
	> MaybeMigration<To, V, EF, EB> for (M1, M2)
{
	type GetWithDefault<Default: Migration<To, V, EF, EB>> =
		M2::GetWithDefault<M1::GetWithDefault<Default>>;
}
