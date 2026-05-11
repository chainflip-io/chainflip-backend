pub mod basics;
pub mod primitives;
pub mod registry;

use self::basics::*;
use crate::migrations::basics::VariantName;

macro_rules! all_runtime_versions {
	($(
		$version:ident => $Migration:ident,
	)*) => {
		// every version is a struct that implements `VariantName`
		$(
			#[derive(Clone, Copy)]
			#[allow(nonstandard_style)]
			pub struct $version;
			impl VariantName for $version {}
		)*

		/// List of all migrations for this type.
		pub trait Migrations:
			HasGenericVariant<
			MigrationFromGeneric: Migration<Self, vCurrent, From: IsHistoricalType<GetCurrentType = Self>>,
		> {
			type DefaultMigration: $(
				Migration<migration_helpers::$version<Self>, $version, From: IsHistoricalType<GetCurrentType = Self>> +
			)*;

			$(
				type $Migration: Migration<migration_helpers::$version<Self>, $version, From: IsHistoricalType<GetCurrentType = Self>> = Self::DefaultMigration;
			)*
		}

		// helper trait implementations to get access to the type at an arbitrary version
		$(
			impl<X: Migrations> HasVersion<$version> for X {
				type HistoricalType = migration_helpers::$version<X>;
				type HistoricalMigration = X::$Migration;
				type MigrationToCurrent = migration_helpers::$Migration<X>;
			}
		)*

		pub mod migration_helpers {
			use super::{Migrations, Migration, vCurrent};
			generate_migration_helpers! { $( $version => $Migration, )*}
		}
	};
}

macro_rules! generate_migration_helpers {
	(
		$old:ident => $OldMigration:ident, $new:ident => $NewMigration:ident, $($rest:tt)*
	) => {
		#[allow(nonstandard_style)]
		pub type $old<M: Migrations> = <M::$NewMigration as Migration<$new<M>, super::$new>>::From;

		pub type $OldMigration<M: Migrations> = (M::$NewMigration, super::$new, $NewMigration<M>);

		generate_migration_helpers!{ $new => $NewMigration, $($rest)*}
	};
	(
		$new:ident => $NewMigration:ident,
	) => {
		#[allow(nonstandard_style)]
		pub type $new<M: Migrations> = <M::MigrationFromGeneric as Migration<M, vCurrent>>::From;

		pub type $NewMigration<M: Migrations> = M::MigrationFromGeneric;
	}
}

all_runtime_versions! {
	v0200 => MigrationTo0200,
	v0201 => MigrationTo0201,
	v0202 => MigrationTo0202,
}
