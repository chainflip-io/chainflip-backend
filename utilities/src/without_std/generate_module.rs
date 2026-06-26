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

#[macro_export]
macro_rules! generate_module {
    // ///////////////////////////////////////////////////////////////////////////////////
    // ////////////////////////////////// Struct /////////////////////////////////////////
    // ///////////////////////////////////////////////////////////////////////////////////
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

            impl< $( $field,)* > Types for ( $($field,)* ) {
                $(
                    type $field = $field;
                )*
            }

            // This has to be used here because of how the `proptest_derive::Arbitrary` derive macro works.
            use sp_std::marker::PhantomData;

            cf_proc_macros::better_modules! {
                mod (Ty: Types) {
                    mod $( $( ($T $(: $TBound)?) )+ )? {

                        #[derive(Hash, codec::Encode, codec::Decode, codec::DecodeWithMemTracking, scale_info::TypeInfo, codec::MaxEncodedLen, Default)]
                        #[derive_where::derive_where(Debug; $(Ty::$field: sp_std::fmt::Debug),*)]
                        #[derive_where(Copy; $(Ty::$field: Copy),*)]
                        #[derive_where(Clone; $(Ty::$field: Clone),*)]
                        #[derive_where(PartialEq; $(Ty::$field: PartialEq),*)]
                        #[derive_where(Eq; $(Ty::$field: Eq),*)]
                        #[cfg_attr(any(test, all(feature = "proptest", feature = "std")), derive(proptest_derive::Arbitrary))]
                        #[scale_info(skip_type_params(Ty))]
                        #[derive(cf_proc_macros::HasTypeIntrospection)]
                        pub struct Struct {
                            $(
                                pub $field: Ty::$field,
                            )*
                            // In order for `proptest_derive::Arbitrary` to work, we're not allowed to mention `sp_std` in the following type,
                            // since the macro has manual filters for `std::marker::PhantomData`, and `PhantomData`, but not for `sp_std::...`.
                            // That's why we import it above.
                            pub _phantom: PhantomData<($($($T,)+)?)>,
                        }

                    }
                }
            }

            impl<$( $($T $(: $TBound)?,)+ )? Ty: Types<$($field: IsHistoricalType,)*>> IsHistoricalType for Struct<Ty, $($($T,)+)?>
                where $struct$(< $($T,)+ >)?: HasChangelog
            {
                type GetCurrentType = $struct$(< $($T,)+ >)?;
            }

            pub type see_field_changelogs = see_field_changelogs_and_also<()>;
            pub struct see_field_changelogs_and_also<M>(M);

            cf_proc_macros::better_modules! {
                mod (To: HistoricalTypesAt<V>) (V: Version)
                {
                    pub trait CustomMigration {
                        $(
                            type $field: MaybeMigration<To::$field, V> = DefaultMigration;
                        )*
                    }

                    impl CustomMigration for () {}

                    impl<M1: CustomMigration, M2: CustomMigration> CustomMigration for (M1, M2) {
                        $(
                            type $field = (M1::$field, M2::$field);
                        )*
                    }
                }
            }


            cf_proc_macros::better_modules! {
                mod (M: CustomMigration<To, V>) (To: HistoricalTypesAt<V>) (V: Version) {

                    mod field_migrations
                    {
                        use super::*;
                        $(
                            pub type $field = <M::$field as MaybeMigration<To::$field, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$field, V>>;
                        )*
                    }

                    mod field_migration_sources
                    {
                        use super::*;
                        $(
                            pub type $field = <field_migrations::$field as Migration<To::$field, V>>::From;
                        )*
                        pub type Types = ( $( $field, )*);
                    }

                    impl<$( $($T $(: $TBound)?,)+ )?> Migration<Struct<To, $($($T,)+)?>, V> for see_field_changelogs_and_also<M>
                    where
                        Struct< field_migration_sources::Types , $($($T,)+)?  >: IsHistoricalType
                    {
                        type From = Struct< field_migration_sources::Types , $($($T,)+)?  >;

                        fn forwards(x: Self::From) -> Struct<To, $($($T,)+)?> {
                            Struct {
                                $(
                                    $field: field_migrations::$field::forwards(x.$field),
                                )*
                                _phantom: Default::default(),
                            }
                        }

                        fn backwards(x: Struct<To, $($($T,)+)?>) -> Self::From {
                            Struct {
                                $(
                                    $field: field_migrations::$field::backwards(x.$field),
                                )*
                                _phantom: Default::default(),
                            }
                        }
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
                )*
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
    };

    // ///////////////////////////////////////////////////////////////////////////////////
    // /////////////////////////////////// Enum //////////////////////////////////////////
    // ///////////////////////////////////////////////////////////////////////////////////
    (
		$(#[$($Attributes:tt)*])*
        $vis:vis enum $enum:ident$(< $($T:ident $(: $TBound:path)?),+ >)? {
            $(
		        $(#[$($Variant_Attributes:tt)*])*
                $variant:ident
                    $( ( $($variant_ty:ty),* ) )?
                    $( { $($variant_field:ident : $variant_field_ty:ty ,)* } )?
                    $(= $variant_discriminant:literal)?
                    ,
            )*
        }
        mod $mod:ident { #![migrations] }
    ) => {

        $(#[$($Attributes)*])*
        $vis enum $enum$(< $($T $(: $TBound)?),+ >)? {
            $(
                $( #[$($Variant_Attributes)*])*
                $variant
                    $( ( $($variant_ty),* ) )?
                    $( { $($variant_field : $variant_field_ty ,)* } )?
                    $(= $variant_discriminant)?
                    ,
            )*
        }

        pub mod $mod {
            #![allow(nonstandard_style)]
            #![allow(unused)]

            use super::*;
            use cf_utilities::migrations::*;
            use cf_utilities::migrations::basics::*;

            ///////////////
            // start generic fibered type helpers
            pub trait Types {
                $(
                    type $variant;
                )*
            }

            pub trait HistoricalTypesAt<V: Version> = Types<
                $(
                    $variant: IsHistoricalTypeAt<V>,
                )*
            >;

            impl< $( $variant,)* > Types for ( $($variant,)* ) {
                $(
                    type $variant = $variant;
                )*
            }

            pub trait CustomMigration<
                To: HistoricalTypesAt<V>,
                V: Version,
            > {
                $(
                    type $variant: MaybeMigration<To::$variant, V> = DefaultMigration;
                )+
            }


            // this extracts the From types (per variant) from a CustomMigration
            #[derive_where::derive_where(Debug, Default; )]
            pub struct source_of_custom_migration<To: HistoricalTypesAt<V>, V: Version, M: CustomMigration<To, V>>(sp_std::marker::PhantomData<(To, V, M)>);
            impl<To: HistoricalTypesAt<V>, V: Version, M: CustomMigration<To, V>> Types for source_of_custom_migration<To, V, M> {
                $(
                    type $variant = <
                        <M::$variant as MaybeMigration<To::$variant, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$variant, V>>
                        as Migration<To::$variant, V>
                    >::From;
                )+
            }

            type ResolveCustomMigration<To: HistoricalTypesAt<V>, V: Version, M: CustomMigration<To, V>> = (
                $(
                    <M::$variant as MaybeMigration<To::$variant, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$variant, V>>,
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
                    type $variant = (M1::$variant, M2::$variant);
                )+
            }

            // end generic fibered type helpers
            ///////////////


            // This has to be used here because of how the `proptest_derive::Arbitrary` derive macro works.
            use sp_std::marker::PhantomData;

            /// This is purely used for backwards compatibility with older runtimes, and won't be exposed on the
            /// rpc layer. So there's intentionally no Serialize/Deserialize implementation
            #[derive(Copy, Clone, PartialEq, Eq, Hash)]
            #[derive_where::derive_where(Debug; $(Ty::$variant: sp_std::fmt::Debug),*)]
            pub enum Enum<Ty: Types, $( $($T $(: $TBound)?,)+ )? > {
                $(
                    $variant(Ty::$variant),
                )*
                _phantom(!, PhantomData<($($($T,)+)?)>),
            }

            // --------------------- custom implemenations of external traits --------------------------

            //
            // TypeInfo
            //
            // Implemented manually so that the variant indices in the type registry match
            // the actual Encode/Decode discriminants (which skip empty variants and respect
            // user-provided discriminant values). The `_phantom` variant is excluded from
            // the type info since it's not a real variant.
            impl<$( $($T: 'static $(+ $TBound)?,)+ )? Ty: Types + 'static> scale_info::TypeInfo for Enum<Ty, $($($T,)+)?>
            where
                $(
                    Ty::$variant: scale_info::TypeInfo + cf_utilities::type_introspection::HasTypeIntrospection + 'static,
                )*
            {
                type Identity = Self;

                fn type_info() -> scale_info::Type {
                    let mut _disc: u8 = 0;
                    let mut variants = scale_info::build::Variants::new();
                    $(
                        if !<Ty::$variant as cf_utilities::type_introspection::HasTypeIntrospection>::is_empty_type() {
                            $( _disc = $variant_discriminant as u8; )?
                            let disc = _disc;
                            variants = variants.variant(stringify!($variant), |v| {
                                v.index(disc)
                                 .fields(scale_info::build::Fields::unnamed()
                                     .field(|f| f.ty::<Ty::$variant>()))
                            });
                            _disc += 1;
                        }
                    )*

                    scale_info::Type::builder()
                        .path(scale_info::Path::new(stringify!(Enum), module_path!()))
                        .variant(variants)
                }
            }

            //
            // Encode / Decode / DecodeWithMemTracking
            //
            // These traits have to be implemented manually because empty variants (containing the `Never` type)
            // must be completely skipped and must not consume discriminant indices.
            //
            // Discriminant handling: when variants have explicit discriminants (`= N`), those values
            // are used as SCALE encoding indices (matching parity-scale-codec's derive behavior).
            // Empty variants are treated as if removed from the source code: they don't consume a
            // discriminant slot, and subsequent implicit discriminants are computed from the last
            // non-empty variant.
            impl<$( $($T $(: $TBound)?,)+ )? Ty: Types> codec::Encode for Enum<Ty, $($($T,)+)?>
            where
                $(
                    Ty::$variant: codec::Encode + cf_utilities::type_introspection::HasTypeIntrospection,
                )*
            {
                fn size_hint(&self) -> usize {
                    match self {
                        $(
                            Self::$variant(val) => 1usize + codec::Encode::size_hint(val),
                        )*
                        Self::_phantom(never, _) => match *never {},
                    }
                }

                fn encode_to<__W: codec::Output + ?Sized>(&self, dest: &mut __W) {
                    let mut _disc: u8 = 0;
                    $(
                        let $variant;
                        if !<Ty::$variant as cf_utilities::type_introspection::HasTypeIntrospection>::is_empty_type() {
                            $( _disc = $variant_discriminant as u8; )?
                            $variant = _disc;
                            _disc += 1;
                        } else {
                            $variant = 0; // dummy value, variant will never be encoded
                        }
                    )*

                    match self {
                        $(
                            Self::$variant(val) => {
                                codec::Encode::encode_to(&$variant, dest);
                                codec::Encode::encode_to(val, dest);
                            }
                        )*
                        Self::_phantom(never, _) => match *never {}
                    }
                }
            }

            impl<$( $($T $(: $TBound)?,)+ )? Ty: Types> codec::Decode for Enum<Ty, $($($T,)+)?>
            where
                $(
                    Ty::$variant: codec::Decode + cf_utilities::type_introspection::HasTypeIntrospection,
                )*
            {
                fn decode<__I: codec::Input>(input: &mut __I) -> Result<Self, codec::Error> {
                    let idx = <u8 as codec::Decode>::decode(input)?;
                    let mut _disc: u8 = 0;
                    $(
                        if !<Ty::$variant as cf_utilities::type_introspection::HasTypeIntrospection>::is_empty_type() {
                            $( _disc = $variant_discriminant as u8; )?
                            if idx == _disc {
                                return Ok(Self::$variant(<Ty::$variant as codec::Decode>::decode(input)?));
                            }
                            _disc += 1;
                        }
                    )*
                    Err(codec::Error::from("Invalid variant index"))
                }
            }

            impl<$( $($T $(: $TBound)?,)+ )? Ty: Types> codec::DecodeWithMemTracking for Enum<Ty, $($($T,)+)?>
            where
                $(
                    Ty::$variant: codec::DecodeWithMemTracking + cf_utilities::type_introspection::HasTypeIntrospection,
                )*
                Self: codec::Decode,
            {}

            impl<$( $($T $(: $TBound)?,)+ )? Ty: Types> codec::MaxEncodedLen for Enum<Ty, $($($T,)+)?>
            where
                $(
                    Ty::$variant: codec::MaxEncodedLen + cf_utilities::type_introspection::HasTypeIntrospection,
                )*
                Self: codec::Encode,
            {
                fn max_encoded_len() -> usize {
                    let mut max_variant_size: usize = 0;
                    $(
                        if !<Ty::$variant as cf_utilities::type_introspection::HasTypeIntrospection>::is_empty_type() {
                            let size = <Ty::$variant as codec::MaxEncodedLen>::max_encoded_len();
                            if size > max_variant_size {
                                max_variant_size = size;
                            }
                        }
                    )*
                    // 1 byte for the discriminant + max variant payload
                    1usize + max_variant_size
                }
            }

            //
            // Arbitrary
            //
            // This trait has to be implemented manually because the standard derive doesn't properly deal with variants that
            // cannot be instantiated (e.g. because they contain `!`, the "Never" type).
            #[cfg(any(test, all(feature = "proptest", feature = "std")))]
            impl<$( $($T: 'static $(+ $TBound)?,)+ )? Ty: Types + 'static> proptest::arbitrary::Arbitrary for Enum<Ty, $($($T,)+)?>
            where
                $(
                    Ty::$variant: proptest::arbitrary::Arbitrary + cf_utilities::type_introspection::HasTypeIntrospection + sp_std::fmt::Debug + 'static,
                )*
            {
                type Parameters = ();
                type Strategy = proptest::strategy::BoxedStrategy<Self>;

                fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
                    use proptest::strategy::Strategy;

                    let mut strategies: Vec<proptest::strategy::BoxedStrategy<Self>> = Vec::new();
                    $(
                        if !<Ty::$variant as cf_utilities::type_introspection::HasTypeIntrospection>::is_empty_type() {
                            strategies.push(
                                proptest::arbitrary::any::<Ty::$variant>()
                                    .prop_map(|val| Enum::$variant(val))
                                    .boxed()
                            );
                        }
                    )*
                    assert!(!strategies.is_empty(), "All variants of Enum are empty types — cannot generate arbitrary values");
                    proptest::strategy::Union::new(strategies).boxed()
                }
            }

            impl<$( $($T $(: $TBound)?,)+ )? Ty: Types<$($variant: IsHistoricalType,)*>> IsHistoricalType for Enum<Ty, $($($T,)+)?>
                where $enum$(< $($T,)+ >)?: HasChangelog
            {
                type GetCurrentType = $enum$(< $($T,)+ >)?;
            }

            pub type see_variant_changelogs = see_variant_changelogs_and_also<()>;
            pub struct see_variant_changelogs_and_also<M>(M);


            impl<M: CustomMigration<To, V>, $( $($T $(: $TBound)?,)+ )? To: HistoricalTypesAt<V>, V: Version> Migration<Enum<To, $($($T,)+)?>, V> for see_variant_changelogs_and_also<M>
            where
                Enum< source_of_custom_migration<To, V, M> , $($($T,)+)?  >: IsHistoricalType
            {
                type From = Enum< source_of_custom_migration<To, V, M> , $($($T,)+)?  >;

                fn forwards(x: Self::From) -> Enum<To, $($($T,)+)?> {
                    match x {
                        $(
                            Enum::$variant(val) => Enum::$variant(<ResolveCustomMigration::<To, V, M> as Types>::$variant::forwards(val)),
                        )*
                        Enum::_phantom(never, _) => Enum::_phantom(never, Default::default()),
                    }
                }

                fn backwards(x: Enum<To, $($($T,)+)?>) -> Self::From {
                    match x {
                        $(
                            Enum::$variant(val) => Enum::$variant(<ResolveCustomMigration::<To, V, M> as Types>::$variant::backwards(val)),
                        )*
                        Enum::_phantom(never, _) => Enum::_phantom(never, Default::default()),
                    }
                }
            }

            // ----------------- predefined migrations ------------------ //


            // ----------------- connection with default enum ------------------ //

            cf_proc_macros::better_modules! {
                mod $( $( ($T $(: $TBound)?) )+ )?
                {
                    type RealEnum = $enum $(< $($T,)+ >)?;

                    pub mod variants {
                        use super::*;
                        pub mod __impls {
                            use super::*;
                            $(
                                pub mod $variant {
                                    use super::*;
                                    $crate::generate_module! {
                                        pub struct variant_struct {
                                            $( pub value: ( $($variant_ty,)* ), )?
                                            $( $( pub $variant_field: $variant_field_ty,)*)?
                                        }
                                        mod variant_mod { #![migrations] }
                                    }
                                    impl HasChangelog for variant_struct
                                    where
                                        $( ($($variant_ty, )*) : HasChangelog )?
                                    {
                                        type if_unspecified = variant_mod::see_field_changelogs;
                                    }
                                }
                            )*
                        }

                        $(
                            pub type $variant = __impls::$variant::variant_struct;

                            impl Into<RealEnum> for $variant {
                                fn into(self) -> RealEnum {
                                    // $( let (cf_utilities::comma_separated_identifiers_for!($($variant_ty)*)) = self; )?
                                    // $enum::variant
                                    //     $(
                                    //         (cf_utilities::comma_separated_identifiers_for!($($variant_ty)*))
                                    //     )?
                                    //     $(
                                    //         $( $variant_field: self.$variant_field, )*
                                    //     )?
                                        $crate::or_else! {
                                            ($(
                                                cf_utilities::tuple_into_enum_variant!(self.value; $enum::$variant; $($variant_ty),*)
                                            )? ) or (
                                                $crate::or_else! {
                                                    ($(
                                                        $enum::$variant {
                                                            $(
                                                                $variant_field: self.$variant_field,
                                                            )*
                                                        }
                                                    )? ) or (
                                                        $enum::$variant
                                                    )
                                                }
                                            )
                                        }
                                }
                            }
                        )*
                    }

                    pub struct DefaultTypes {}

                    impl Types for DefaultTypes {
                        $(
                            type $variant = variants::$variant;
                        )*
                    }

                    impl HasGenericVariant for RealEnum
                        where
                            $(
                                $( ($($variant_ty, )*) : HasChangelog ,)?
                                $( ($($variant_ty, )*) : HasGenericVariant<GenericType: IsHistoricalType> ,)?
                            )*
                            Enum<(
                                $(
                                    GetGenericVariant<variants::$variant>,
                                )*
                            ), $($($T,)+)?>: IsHistoricalType
                    {
                        type GenericType = Enum<(
                            $(
                                GetGenericVariant<variants::$variant>,
                            )*
                        ), $($($T,)+)?>;
                        type MigrationFromGeneric = GlobalMigrationFromGeneric;
                    }


                    impl Migration<RealEnum, vCurrent> for GlobalMigrationFromGeneric
                    where
                            $(
                                $( ($($variant_ty, )*) : HasChangelog ,)?
                                $( ($($variant_ty, )*) : HasGenericVariant<GenericType: IsHistoricalType> ,)?
                            )*
                        Enum<(
                            $(
                                GetGenericVariant<variants::$variant>,
                            )*
                        ), $($($T,)+)?>: IsHistoricalType
                    {
                        type From = Enum<(
                            $(
                                GetGenericVariant<variants::$variant>,
                            )*
                        ), $($($T,)+)?>;

                        fn forwards(x: Self::From) -> RealEnum {
                            match x {
                                $(
                                    Enum::$variant(val) =>
                                        (<<variants::$variant as HasGenericVariant>::MigrationFromGeneric as Migration<variants::$variant, vCurrent>>::forwards(val)).into(),
                                )*
                                Enum::_phantom(never, _) => match never {},
                            }
                        }

                        fn backwards(x: RealEnum) -> Self::From {
                            x.elim(
                                $(
                                    |x| Enum::$variant(<<variants::$variant as HasGenericVariant>::MigrationFromGeneric as Migration<variants::$variant, vCurrent>>::backwards(
                                        variants::$variant {
                                            $(value: x as ($($variant_ty, )*),)?
                                            $( $( $variant_field: x.$variant_field, )*)?
                                        }
                                    )),
                                )*
                            )
                        }
                    }

                }
            }

            // pub struct DefaultTypes$(< $($T $(: $TBound)?),+ >)?($($($T,)+)?);


        }
    };
}
pub use generate_module;
