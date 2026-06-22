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

#![allow(clippy::allow_attributes)]

pub mod basics;
pub mod primitives;

use self::basics::*;
use crate::migrations::basics::Version;
use CanonicalPatchVersion::*;

macro_rules! define_all_runtime_versions {
	($(
		{
			release: $version:ident,
			canonical_patch: $canonical_patch_version:expr,
			changelog_entry: $Migration:ident,
		},
	)*) => {
		// every version is a struct that implements `Version`
		$(
			#[derive(Clone, Copy)]
			#[allow(nonstandard_style)]
			pub struct $version;
			impl Version for $version {
				const CANONICAL_RUNTIME_PATCH_VERSION_FOR_COMPATIBILITY_TEST: Option<CanonicalPatchVersion> = Some($canonical_patch_version);
			}
		)*

        /// List of all historical changes (migrations) for this type.
        ///
        /// ## Associated types:
        ///
        /// This trait is the foundation of auto-generated migrations. It provides all the information
        /// required to refer to historical versions of a type, as well as migrating data forwards and backwards in time.
        ///
        /// It has an associated type for every release describing the changes that happened in that release
        ///  - `type in_20100`: the migration that transformed this type in release 2.1
        ///  - `type in_20000`: the migration that transformed this type in release 2.0
        ///  - ... and so on
        ///
        /// Each of these entries has to implement `Migration<{Type at this version}, v{VERSION}`>`. This means
        /// that the migration has to target the correct historical version of this type at that point in time.
        ///
        /// It also has the following associated type:
        ///  - `type if_unspecified`: this is the default migration that should be used if nothing is specified for
        ///    a release version.
        ///
        /// ## Accessing historical types
        ///
        /// If a type X implements `HasChangelog`, all of its historical versions can be easily referenced:
        ///  - Use `<X as HasVersion<{VERSION}>>::HistoricalType` to access the historical version of X.
        ///    (Versions look like `v20000`, `v20100`, etc.)
        ///  - Use `migrate_from_historical_type` to convert a type from historical to the current version.
        ///  - Use `migrate_to_historical_type` to convert a current type to a historical version.
        ///
        /// ## Macros
        ///
        /// There's the macro `#[cf_proc_macros::generate_module]` that can be used to generate most of the
        /// pre-reqs of `HasChangelog`. For example, using that, the implementation for RpcAccountInfoCommonItems looks
        /// mostly like this:
        ///
        /// ```ignore
        /// #[cf_proc_macros::generate_module]
        /// pub struct RpcAccountInfoCommonItems<Balance> {
        ///     ...
        /// }
        /// impl<Balance: HasChangelog> HasChangelog for RpcAccountInfoCommonItems<Balance>
        /// {
        ///     type if_unspecified = _RpcAccountInfoCommonItems::see_field_changelogs;
        ///     type in_20200 = _RpcAccountInfoCommonItems::see_field_changelogs_and_also<
        ///         _RpcAccountInfoCommonItems::field::account_id::Added,
        ///     >;
        /// }
        /// ```
        ///
        /// ## Migration sequence
        ///
        /// The migrations specified in the changelog form a sequence. Every migration targets the `Migration::From` type
        /// of the chronologically next migration.
        ///
        /// Note that the target of the latest migration is *not* `Self`. There is another trait called `HasGenericVariant`
        /// which is an "intermediate" between the latest migration and the real `Self`. The migration sequence looks like this:
        ///
        ///  --[migration: Self::in_20000]-> `<Self as HasVersion<v20000>>::HistoricalType`
        ///  --[migration: Self::in_20100]-> `<Self as HasVersion<v20100>>::HistoricalType`
        ///  --[migration: Self::in_20200]-> `<Self as HasVersion<v20200>>::HistoricalType` (equal to `Self::GenericType`)
        ///  --[migration: Self::MigrationFromGeneric]-> `Self`
        ///
        /// In order to implement `HasChangelog` a type also has to implement `HasGenericVariant`.
        ///
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
		macro_rules! for_each_runtime_version {
			($$macro_name:ident) => {
				$(
					$$macro_name!{ $version }
				)*
			}
		}
		pub use for_each_runtime_version;
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

// All major runtime versions.
// The table uses the following format:
// 1. `release: vMajorMinor00`: this describes the chainflip release version. E.g. chainflip release
//    2.1 is represented by v20100.
// 2. `canonical_patch: Released(MajorMinorPatch) | Unreleased`: this is the exact patch of that
//    chainflip release which should be used as canonical runtime providing the metadata which is
//    tested against for historical type compatibility tests. For every canonical `Released(patch)`
//    with `patch` >= 20100 that's listed here, there should be a metadata file called
//    `runtime_{canonical_patch}.scale` located in
//    `state-chain/cf-integration-tests/historical_metadata`. It can be downloaded using the script
//    in `bouncer/commands/download_metadata.ts`. Use `Unreleased` if this version isn't released
//    yet.
// 3. `changelog_entry: in_MajorMinor00`: this should match up with the first entry, and is the name
//    of the changelog entry (in the `HasChangelog` type) for this release.
define_all_runtime_versions! {
	{
		release: v20000,
		canonical_patch: Released(20012),
		changelog_entry: in_20000,
	},
	{
		release: v20100,
		canonical_patch: Released(20119),
		changelog_entry: in_20100,
	},
	{
		release: v20200,
		canonical_patch: Released(20203),
		changelog_entry: in_20200,
	},
	{
		release: v20300,
		canonical_patch: Unreleased,
		changelog_entry: in_20300,
	},
}
