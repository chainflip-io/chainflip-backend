#![allow(clippy::allow_attributes)]

pub mod basics;
pub mod primitives;

use self::basics::*;
use crate::migrations::basics::VariantName;

macro_rules! all_runtime_versions {
	($(
		$version:ident ($latest_patch:literal) => $Migration:ident,
	)*) => {
		// every version is a struct that implements `VariantName`
		$(
			#[derive(Clone, Copy)]
			#[allow(nonstandard_style)]
			pub struct $version;
			impl VariantName for $version {
				const LATEST_RUNTIME_PATCH_VERSION: u32 = $latest_patch;
			}
		)*

		/// List of all HasChangelog for this type.
		pub trait HasChangelog:
			HasGenericVariant<
			MigrationFromGeneric: Migration<Self, vCurrent, From: IsHistoricalType<GetCurrentType = Self>>,
		> {
			#[allow(nonstandard_style)]
			type if_unspecified: $(
				Migration<migration_helpers::$version<Self>, $version, From: IsHistoricalType<GetCurrentType = Self>> +
			)*;

			$(
				#[allow(nonstandard_style)]
				type $Migration: Migration<migration_helpers::$version<Self>, $version, From: IsHistoricalType<GetCurrentType = Self>> = Self::if_unspecified;
			)*
		}

		pub trait OrdMigrations = HasChangelog<
			MigrationFromGeneric: Migration<Self, vCurrent, From: Ord + IsHistoricalType<GetCurrentType = Self>>,

			if_unspecified: $(
				Migration<migration_helpers::$version<Self>, $version, From: Ord + IsHistoricalType<GetCurrentType = Self>> +
			)*,

			$(
				$Migration: Migration<migration_helpers::$version<Self>, $version, From: Ord + IsHistoricalType<GetCurrentType = Self>>,
			)*
		>;

		// helper trait implementations to get access to the type at an arbitrary version
		$(
			impl<X: HasChangelog> HasVersion<$version> for X {
				type HistoricalType = migration_helpers::$version<X>;
				type HistoricalMigration = X::$Migration;
				type MigrationToCurrent = migration_helpers::$Migration<X>;
			}
		)*

		pub mod migration_helpers {
			use super::{HasChangelog, Migration, vCurrent};
			generate_migration_helpers! { $( $version => $Migration, )*}
		}
	};
}

macro_rules! generate_migration_helpers {
	(
		$old:ident => $OldMigration:ident, $new:ident => $NewMigration:ident, $($rest:tt)*
	) => {
		#[allow(nonstandard_style)]
		pub type $old<M: HasChangelog> = <M::$NewMigration as Migration<$new<M>, super::$new>>::From;

		#[allow(nonstandard_style)]
		pub type $OldMigration<M: HasChangelog> = (M::$NewMigration, super::$new, $NewMigration<M>);

		generate_migration_helpers!{ $new => $NewMigration, $($rest)*}
	};
	(
		$new:ident => $NewMigration:ident,
	) => {
		#[allow(nonstandard_style)]
		pub type $new<M: HasChangelog> = <M::MigrationFromGeneric as Migration<M, vCurrent>>::From;

		#[allow(nonstandard_style)]
		pub type $NewMigration<M: HasChangelog> = M::MigrationFromGeneric;
	}
}

all_runtime_versions! {
	v0200 (20012) => in_20000,
	v0201 (20119) => in_20100,
	v0202 (20201) => in_20200,
}
