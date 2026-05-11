// ---------- definition of migrations ------------

use crate::migrations::Migrations;

pub trait VariantName: Copy {}

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

	fn forwards(x: Self::From) -> T {
		Default::default()
	}

	fn backwards(x: T) -> Self::From {
		()
	}
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
#[allow(nonstandard_style)]
pub struct vCurrent;
impl VariantName for vCurrent {}

pub trait HasGenericVariant: Sized {
	type MigrationFromGeneric: Migration<Self, vCurrent>;
}

pub type GetGenericVariant<X: HasGenericVariant> =
	<X::MigrationFromGeneric as Migration<X, vCurrent>>::From;

pub struct GlobalMigrationFromGeneric;
