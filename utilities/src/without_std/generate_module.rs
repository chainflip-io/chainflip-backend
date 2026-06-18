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

// EXPLORATORY (2.3): this macro previously supported only structs with zero or one
// generic type parameter (`$(<$T:ident ...>)?`). It has been generalised to accept
// any number of generic parameters (`$(< $($T:ident ...),+ >)?`) so that types like
// `RpcLoanAccount<AccountId, Amount>` and `RpcLoan<AccountId, Amount>` can be onboarded.
// The change is purely mechanical: every place that used the single `$T` now expands the
// `$($T),*` repetition (with a trailing comma where it's emitted into type/tuple position).
#[macro_export]
macro_rules! generate_module {
    (
		$(#[$($Attributes:tt)*])*
        $vis:vis struct $struct:ident$(< $($T:ident $(: $TBound:path)?),+ >)? {
            $(
		        $(#[$($Field_Attributes:tt)*])*
                $field_vis:vis $field:ident: $field_ty:ty,
            )*
        }
        mod $mod:ident { #![migrations] }
    ) => {
        $(
            #[$($Attributes)*]
        )*
        $vis struct $struct$(< $($T $(: $TBound)?),+ >)? {
            $(
                $( #[$($Field_Attributes)*])*
                $field_vis $field: $field_ty,
            )*
        }

        pub mod $mod {
            #![allow(nonstandard_style)]
            #![allow(unused)]

            use super::*;
            use cf_utilities::migrations::*;
            use cf_utilities::migrations::basics::*;

            pub trait Types {
                $(
                    type $field;
                )*
            }
            pub trait HistoricalTypesAt<V: Version> = Types<
                $(
                    $field: IsHistoricalTypeAt<V>,
                )*
            >;
            pub trait DebugTypes = Types<
                $(
                    $field: sp_std::fmt::Debug,
                )+
            >;

            impl< $( $field,)* > Types for ( $($field,)* ) {
                $(
                    type $field = $field;
                )*
            }

            pub trait CustomMigration<
                To: HistoricalTypesAt<V>,
                V: Version,
            > {
                $(
                    type $field: MaybeMigration<To::$field, V> = DefaultMigration;
                )+
            }

            // this extracts the From types (per field) from a CustomMigration
            // EXPLORATORY (2.3): added `Default` (no generic bounds - it's just PhantomData).
            // Needed once a release becomes a non-latest version in the chain, so that the
            // historical `Struct<source_of_custom_migration<..>>` can satisfy `Default`.
            #[derive_where::derive_where(Debug, Default; )]
            pub struct source_of_custom_migration<To: HistoricalTypesAt<V>, V: Version, M: CustomMigration<To, V>>(sp_std::marker::PhantomData<(To, V, M)>);
            impl<To: HistoricalTypesAt<V>, V: Version, M: CustomMigration<To, V>> Types for source_of_custom_migration<To, V, M> {
                $(
                    type $field = <
                        <M::$field as MaybeMigration<To::$field, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$field, V>>
                        as Migration<To::$field, V>
                    >::From;
                )+
            }

            type ResolveCustomMigration<To: HistoricalTypesAt<V>, V: Version, M: CustomMigration<To, V>> = (
                $(
                    <M::$field as MaybeMigration<To::$field, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$field, V>>,
                )+
            );

            impl <
                To: HistoricalTypesAt<V>,
                V: Version
            > CustomMigration<To, V> for () {}

            impl<To: HistoricalTypesAt<V>, V: Version, M1: CustomMigration<To, V>, M2: CustomMigration<To, V>>
            CustomMigration<To,V>
            for (M1, M2)
            {
                $(
                    type $field = (M1::$field, M2::$field);
                )+
            }

            // This has to be used here because of how the `proptest_derive::Arbitrary` derive macro works.
            use sp_std::marker::PhantomData;

            /// This is purely used for backwards compatibility with older runtimes, and won't be exposed on the
            /// rpc layer. So there's intentionally no Serialize/Deserialize implementation
            #[derive(Copy, Clone, PartialEq, Eq, Hash, Encode, Decode, codec::DecodeWithMemTracking, TypeInfo, codec::MaxEncodedLen, Default)]
            #[derive_where::derive_where(Debug; $(Ty::$field: sp_std::fmt::Debug),*)]
            #[cfg_attr(any(test, all(feature = "proptest", feature = "std")), derive(proptest_derive::Arbitrary))]
            #[scale_info(skip_type_params(Ty))]
            pub struct Struct<Ty: Types, $( $($T $(: $TBound)?,)+ )? > {
                $(
                    pub $field: Ty::$field,
                )+
                // In order for `proptest_derive::Arbitrary` to work, we're not allowed to mention `sp_std` in the following type,
                // since the macro has manual filters for `std::marker::PhantomData`, and `PhantomData`, but not for `sp_std::...`.
                // That's why we import it above.
                pub _phantom: PhantomData<($($($T,)+)?)>,
            }

            impl<$( $($T $(: $TBound)?,)+ )? Ty: Types<$($field: IsHistoricalType,)*>> IsHistoricalType for Struct<Ty, $($($T,)+)?>
            where $struct$(< $($T,)+ >)?: HasChangelog
            {
                type GetCurrentType = $struct$(< $($T,)+ >)?;
            }

            pub type see_field_changelogs = see_field_changelogs_and_also<()>;
            pub struct see_field_changelogs_and_also<M>(M);

            impl<M: CustomMigration<To, V>, $( $($T $(: $TBound)?,)+ )? To: HistoricalTypesAt<V>, V: Version> Migration<Struct<To, $($($T,)+)?>, V> for see_field_changelogs_and_also<M>
            where
                Struct< source_of_custom_migration<To, V, M> , $($($T,)+)?  >: IsHistoricalType
            {
                type From = Struct< source_of_custom_migration<To, V, M> , $($($T,)+)?  >;

                fn forwards(x: Self::From) -> Struct<To, $($($T,)+)?> {
                    Struct {
                        $(
                            $field: <ResolveCustomMigration::<To, V, M> as Types>::$field::forwards(x.$field),
                        )+
                        _phantom: Default::default(),
                    }
                }

                fn backwards(x: Struct<To, $($($T,)+)?>) -> Self::From {
                    Struct {
                        $(
                            $field: <ResolveCustomMigration::<To, V, M> as Types>::$field::backwards(x.$field),
                        )+
                        _phantom: Default::default(),
                    }
                }

            }

            // ----------------- predefined migrations ------------------ //
            pub mod field {
                $(
                    pub mod $field {
                        use super::super::{OverrideMigrationWith, Version, HistoricalTypesAt, CustomMigration, NewFieldWithDefault};

                        #[derive(Debug)]
                        pub struct Added;
                        impl<V: Version, TargetFieldsTypes: HistoricalTypesAt<V, $field: Default>>
                            CustomMigration<TargetFieldsTypes, V> for Added
                        {
                            type $field = OverrideMigrationWith<NewFieldWithDefault>;
                        }
                    }
                )+
            }

            // ----------------- connection with default struct ------------------ //

            pub struct DefaultTypes$(< $($T $(: $TBound)?),+ >)?($($($T,)+)?);

            impl$(< $($T $(: $TBound)?),+ >)? Types for DefaultTypes$(< $($T,)+ >)? {
                $(
                    type $field = $field_ty;
                )*
            }

            impl $(< $($T $(: $TBound)?),+ >)? HasGenericVariant for $struct $(< $($T,)+ >)?
            where $( $field_ty: HasGenericVariant,)*
                Struct<(
                    $(
                        GetGenericVariant<$field_ty>,
                    )*
                ), $($($T,)+)?>: IsHistoricalType
            {
                type GenericType = Struct<(
                    $(
                        GetGenericVariant<$field_ty>,
                    )*
                ), $($($T,)+)?>;
                type MigrationFromGeneric = GlobalMigrationFromGeneric;
            }

            impl $(< $($T $(: $TBound)?),+ >)? Migration<$struct $(< $($T,)+ >)?, vCurrent> for GlobalMigrationFromGeneric
            where $( $field_ty: HasGenericVariant,)*
                Struct<(
                    $(
                        GetGenericVariant<$field_ty>,
                    )*
                ), $($($T,)+)?>: IsHistoricalType
            {
                type From = Struct<(
                    $(
                        GetGenericVariant<$field_ty>,
                    )*
                ), $($($T,)+)?>;

                fn forwards(x: Self::From) -> $struct $(< $($T,)+ >)? {
                    $struct {
                        $(
                            $field: <<$field_ty as HasGenericVariant>::MigrationFromGeneric as Migration<$field_ty, vCurrent>>::forwards(x.$field),
                        )*
                    }
                }

                fn backwards(x: $struct $(< $($T,)+ >)?) -> Self::From {
                    Struct {
                        $(
                            $field: <<$field_ty as HasGenericVariant>::MigrationFromGeneric as Migration<$field_ty, vCurrent>>::backwards(x.$field),
                        )*
                        _phantom: Default::default(),
                    }
                }
            }
        }
    }
}
pub use generate_module;
