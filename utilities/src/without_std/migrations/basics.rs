// ---------- definition of migrations ------------

use crate::migrations::Migrations;

pub trait VariantName: Copy {
	const LATEST_RUNTIME_PATCH_VERSION: u32;
}

pub trait Migration<To: ?Sized, V: VariantName> {
	type From: IsHistoricalType;
	fn forwards(x: Self::From) -> To;
	fn backwards(x: To) -> Self::From;
}

pub trait HasVersion<V: VariantName> {
	type HistoricalType;
	type HistoricalMigration: Migration<Self::HistoricalType, V>;
	type MigrationToCurrent: Migration<Self, vCurrent, From = Self::HistoricalType>;
}

pub fn migrate_from_historical_type<V: VariantName, X: HasVersion<V>>(
	_v: V,
	x: X::HistoricalType,
) -> X {
	X::MigrationToCurrent::forwards(x)
}

pub fn migrate_to_historical_type<V: VariantName, X: HasVersion<V>>(
	_v: V,
	x: X,
) -> X::HistoricalType {
	X::MigrationToCurrent::backwards(x)
}
// -------- identity migration --------
pub struct IdentityMigration;

impl<X: IsHistoricalType, V: VariantName> Migration<X, V> for IdentityMigration {
	type From = X;

	fn forwards(x: Self::From) -> X {
		x
	}

	fn backwards(x: X) -> Self::From {
		x
	}
}

// -------- composition of migrations --------
impl<V: VariantName, W: VariantName, X, A: Migration<B::From, W>, B: Migration<X, V>>
	Migration<X, V> for (A, W, B)
{
	type From = A::From;

	fn forwards(x: Self::From) -> X {
		B::forwards(A::forwards(x))
	}

	fn backwards(x: X) -> Self::From {
		A::backwards(B::backwards(x))
	}
}

// ------- migration for new field with default value --------

pub struct NewFieldWithDefault;
impl<T: Default, V: VariantName> Migration<T, V> for NewFieldWithDefault {
	type From = ();

	fn forwards(_x: Self::From) -> T {
		Default::default()
	}

	fn backwards(_x: T) -> Self::From {}
}

// ----------- lookups ------------

pub trait IsHistoricalType {
	type GetCurrentType: Migrations;
}
pub trait IsHistoricalTypeAt<V: VariantName> =
	IsHistoricalType<GetCurrentType: HasVersion<V, HistoricalType = Self>>;
pub type GetMigrationToHistoricalType<X: IsHistoricalTypeAt<V>, V: VariantName> =
	<X::GetCurrentType as HasVersion<V>>::HistoricalMigration;

// ----------- associated generic type --------------

#[derive(Clone, Copy)]
#[expect(nonstandard_style)]
pub struct vCurrent;
impl VariantName for vCurrent {
	// TODO this should be synchronized with the one in runtime/lib.rs
	const LATEST_RUNTIME_PATCH_VERSION: u32 = 20201;
}

pub trait HasGenericVariant: Sized {
	type GenericType;
	type MigrationFromGeneric: Migration<Self, vCurrent, From = Self::GenericType>;
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

pub trait MaybeMigration<To, V: VariantName> {
	type GetWithDefault<Default: Migration<To, V>>: Migration<To, V>;
}

pub struct DefaultMigration;
impl<To, V: VariantName> MaybeMigration<To, V> for DefaultMigration {
	type GetWithDefault<Default: Migration<To, V>> = Default;
}

pub struct OverrideMigrationWith<M>(M);
impl<To, V: VariantName, M: Migration<To, V>> MaybeMigration<To, V> for OverrideMigrationWith<M> {
	type GetWithDefault<Default: Migration<To, V>> = M;
}

impl<To, V: VariantName, M1: MaybeMigration<To, V>, M2: MaybeMigration<To, V>> MaybeMigration<To, V>
	for (M1, M2)
{
	type GetWithDefault<Default: Migration<To, V>> =
		M2::GetWithDefault<M1::GetWithDefault<Default>>;
}
