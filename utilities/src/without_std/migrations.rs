#![allow(clippy::allow_attributes)]

pub mod basics;
pub mod primitives;

use self::basics::*;
use crate::migrations::basics::Version;

macro_rules! define_all_released_runtime_versions {
	($(
		{
			release: $version:ident,
			canonical_patch: $latest_patch:literal,
			changelog_entry: $Migration:ident,
		},
	)*) => {
		// every version is a struct that implements `Version`
		$(
			#[derive(Clone, Copy)]
			#[allow(nonstandard_style)]
			pub struct $version;
			impl Version for $version {
				const CANONICAL_RUNTIME_PATCH_VERSION_FOR_COMPATIBILITY_TEST: Option<u32> = Some($latest_patch);
			}
		)*

		/// List of all historical changes (migrations) for this type.
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

		#[macro_export]
		macro_rules! for_each_released_runtime_version {
			($$macro_name:ident) => {
				$(
					$$macro_name!{ $version }
				)*
			}
		}
		pub use for_each_released_runtime_version;
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

// All major runtime versions that have been released to at least one testnet.
// The table uses the following format:
// 1. `release: vMajorMinor00`: this describes the chainflip release version. E.g. chainflip release
//    2.1 is represented by v20100.
// 2. `canonical_patch: MajorMinorPatch`: this is the exact patch of that chainflip release which
//    should be used as canonical runtime providing the metadata which is tested against for
//    historical type compatibility tests.
// 3. `changelog_entry: in_MajorMinor00`: this should match up with the first entry, and is the name
//    of the changelog entry (in the `HasChangelog` type) for this release.
define_all_released_runtime_versions! {
	{
		release: v20000,
		canonical_patch: 20012,
		changelog_entry: in_20000,
	},
	{
		release: v20100,
		canonical_patch: 20119,
		changelog_entry: in_20100,
	},
	{
		release: v20200,
		canonical_patch: 20203,
		changelog_entry: in_20200,
	},
}
