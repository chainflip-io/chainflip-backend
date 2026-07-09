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
                $field_vis:vis $field:ident: $field_ty:ty
            ),*
            $(,)?
        }
        mod $mod:ident { #![migrations] }
    ) => {
        $(
            #[$($Attributes)*]
        )*
        #[derive(cf_proc_macros::IntroElim)]
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

            cf_proc_macros::generic_modules! {
                mod $( $( ($T $(: $TBound)?) )+ )? {
                    mod (Ty: Types) {

                        #[derive(Hash, codec::Encode, codec::Decode, codec::DecodeWithMemTracking, scale_info::TypeInfo, codec::MaxEncodedLen, Default)]
                        #[derive_where::derive_where(Debug; $(Ty::$field: sp_std::fmt::Debug),*)]
                        #[derive_where(Copy; $(Ty::$field: Copy),*)]
                        #[derive_where(Clone; $(Ty::$field: Clone),*)]
                        #[derive_where(PartialEq; $(Ty::$field: PartialEq),*)]
                        #[derive_where(Eq; $(Ty::$field: Eq),*)]
                        #[scale_info(skip_type_params(Ty))]
                        #[derive(cf_proc_macros::HasTypeIntrospection)]
                        #[derive(cf_proc_macros::IntroElim)]
                        #[cfg_attr(any(test, all(feature = "proptest", feature = "std")), derive(cf_proc_macros::ArbitraryWithBounds))]
                        #[cfg_attr(any(test, all(feature = "proptest", feature = "std")), arbitrary(bound(Ty: 'static, $( $($T: 'static,)+ )? $( Ty::$field: sp_std::fmt::Debug + proptest::arbitrary::Arbitrary + 'static ),* )))]

                        pub struct Struct {
                            $(
                                pub $field: Ty::$field,
                            )*
                        }

                        impl IsHistoricalType for Struct where
                            $( Ty::$field: IsHistoricalType,)*
                        {
                            type GetCurrentType = $struct$(< $($T,)+ >)?;
                        }
                    }

                    // ----------------- connection with default struct ------------------ //
                    pub type UserStruct = $struct $(< $($T,)+ >)?;

                    impl HasGenericVariant for $struct $(< $($T,)+ >)? // UserStruct
                    where
                        $( $field_ty: HasChangelog, )*
                        $( $field_ty: HasGenericVariant<GenericType: IsHistoricalType>, )*
                        Struct<( $( < $field_ty as HasGenericVariant>::GenericType,)*)>: IsHistoricalType
                    {
                        type GenericType = Struct<(
                            $(
                                < $field_ty as HasGenericVariant>::GenericType,
                            )*
                        )>;
                        type MigrationFromGeneric = GlobalMigrationFromGeneric;
                    }

                    impl Migration<$struct $(< $($T,)+ >)?, vCurrent> for GlobalMigrationFromGeneric
                    where
                        $( $field_ty: HasChangelog, )*
                        $( $field_ty: HasGenericVariant<GenericType: IsHistoricalType>, )*
                        Struct<( $( < $field_ty as HasGenericVariant>::GenericType,)*)>: IsHistoricalType
                    {
                        type From = Struct<(
                            $(
                                < $field_ty as HasGenericVariant>::GenericType,
                            )*
                        )>;
                        type ForwardsError = cf_utilities::never::Never;
                        type BackwardsError = cf_utilities::never::Never;

                        fn try_forwards(x: Self::From) -> Result<$struct $(< $($T,)+ >)?, Self::ForwardsError> {
                            Ok(
                                $struct {
                                    $(
                                        $field: <<$field_ty as HasGenericVariant>::MigrationFromGeneric as Migration<$field_ty, vCurrent>>::try_forwards(x.$field)?,
                                    )*
                                }
                            )
                        }

                        fn try_backwards(x: $struct $(< $($T,)+ >)?) -> Result<Self::From, Self::BackwardsError> {
                            Ok(
                                Struct::intro(
                                    $(
                                        <<$field_ty as HasGenericVariant>::MigrationFromGeneric as Migration<$field_ty, vCurrent>>::try_backwards(x.$field)?,
                                    )*
                                    Default::default(),
                                )
                            )
                        }
                    }
                }
            }


            pub type see_field_changelogs = see_field_changelogs_and_also<()>;
            pub struct see_field_changelogs_and_also<M>(M);

            cf_proc_macros::generic_modules! {
                mod (To: HistoricalTypesAt<V>) (V: Version)
                {
                    pub trait FieldCustomMigration {
                        $(
                            type $field: MaybeMigration<To::$field, V> = DefaultMigration;
                        )*
                    }

                    impl FieldCustomMigration for () {}

                    impl<M1: FieldCustomMigration, M2: FieldCustomMigration> FieldCustomMigration for (M1, M2) {
                        $(
                            type $field = (M1::$field, M2::$field);
                        )*
                    }

                    mod (M: FieldCustomMigration<To, V>)
                    {
                        mod field_migrations {
                            use super::*;
                            $(
                                pub type $field = <M::$field as MaybeMigration<To::$field, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$field, V>>;
                            )*
                            pub type TyFrom = (
                                $(
                                    <field_migrations::$field as Migration<To::$field, V>>::From,
                                )*
                            );

                            #[derive(cf_proc_macros::IntroElim)]
                            pub enum StructForwardsError {
                                $(
                                    $field(<$field as Migration<To::$field, V>>::ForwardsError),
                                )*
                            }

                            #[derive(cf_proc_macros::IntroElim)]
                            pub enum StructBackwardsError {
                                $(
                                    $field(<$field as Migration<To::$field, V>>::BackwardsError),
                                )*
                            }

                            impl cf_utilities::never::IsEmptyType for StructForwardsError
                                where $(
                                    <$field as Migration<To::$field, V>>::ForwardsError: cf_utilities::never::IsEmptyType,
                                )*
                            {
                                fn as_never(&self) -> cf_utilities::never::Never {
                                    self.elim_ref(
                                        $(
                                            |err: &<$field as Migration<To::$field, V>>::ForwardsError| err.as_never(),
                                        )*
                                        |never, _phantom| never.as_never(),
                                    )
                                }
                            }

                            impl cf_utilities::never::IsEmptyType for StructBackwardsError
                                where $(
                                    <$field as Migration<To::$field, V>>::BackwardsError: cf_utilities::never::IsEmptyType,
                                )*
                            {
                                fn as_never(&self) -> cf_utilities::never::Never {
                                    self.elim_ref(
                                        $(
                                            |err: &<$field as Migration<To::$field, V>>::BackwardsError| err.as_never(),
                                        )*
                                        |never, _phantom| never.as_never(),
                                    )
                                }
                            }

                            mod $( $( ($T $(: $TBound)?) )+ )? {
                                pub type StructVariant<Target: Types> = Struct<$($($T,)+)? Target>;

                                impl Migration<Struct<$($($T,)+)? To>, V> for see_field_changelogs_and_also<M> where
                                    StructVariant<TyFrom>: IsHistoricalType,
                                    $struct<$($($T,)+)?>: HasChangelog
                                {
                                    type ForwardsError = StructForwardsError;
                                    type BackwardsError = StructBackwardsError;

                                    type From = StructVariant<TyFrom>;

                                    fn try_forwards(x: Self::From) -> Result<StructVariant<To>, Self::ForwardsError> {
                                        Ok(Struct::intro(
                                            $(
                                                $field::try_forwards(x.$field)
                                                    .map_err(StructForwardsError::$field)?,
                                            )*
                                            Default::default(),
                                        ))
                                    }

                                    fn try_backwards(x: StructVariant<To>) -> Result<Self::From, Self::BackwardsError> {
                                        Ok(Struct::intro(
                                            $(
                                                $field::try_backwards(x.$field)
                                                    .map_err(StructBackwardsError::$field)?,
                                            )*
                                            Default::default(),
                                        ))
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ----------------- predefined migrations ------------------ //
            pub mod field {
                $(
                    pub mod $field {
                        use super::super::{OverrideMigrationWith, Version, HistoricalTypesAt, FieldCustomMigration, NewFieldWithDefault,
                            HasVersion, Migration, RemovedFieldWithDefault
                        };

                        #[derive(Debug)]
                        pub struct Added;
                        impl<V: Version, TargetFieldsTypes: HistoricalTypesAt<V, $field: Default>>
                            FieldCustomMigration<TargetFieldsTypes, V> for Added
                        {
                            type $field = OverrideMigrationWith<NewFieldWithDefault>;
                        }

                        #[derive(Debug)]
                        pub struct Removed<T>(T);
                        impl<V: Version, TargetFieldsTypes: HistoricalTypesAt<V, $field = ()>, T: HasVersion<V>>
                            FieldCustomMigration<TargetFieldsTypes, V> for Removed<T>
                        where
	                        <T::HistoricalMigration as Migration<T::HistoricalType, V>>::From: Default,
                        {
                            type $field = OverrideMigrationWith<RemovedFieldWithDefault<T>>;
                        }

                        #[derive(Debug)]
                        pub struct CustomMigration<M>(M);
                        impl<V: Version, TargetFieldsTypes: HistoricalTypesAt<V>,
                            M: Migration<TargetFieldsTypes::$field, V>
                        >
                            FieldCustomMigration<TargetFieldsTypes, V> for CustomMigration<M>
                        {
                            type $field = OverrideMigrationWith<M>;
                        }
                    }
                )*
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
                    $( ( $($variant_tuple_entry:ident : $variant_ty:ty),* ) )?
                    $( { $($variant_field:ident : $variant_field_ty:ty ,)* } )?
                    $(= $variant_discriminant:literal)?
                    ,
            )*
        }
        mod $mod:ident { #![migrations] }
    ) => {

        $(#[$($Attributes)*])*
        #[derive(cf_proc_macros::IntroElim)]
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

            cf_proc_macros::generic_modules! {
                mod $( $( ($T $(: $TBound)?) )+ )? {
                    mod (Ty: Types) {

                        #[derive(Copy, Clone, PartialEq, Eq, Hash)]
                        #[derive_where::derive_where(Debug; $(Ty::$variant: sp_std::fmt::Debug),*)]
                        #[derive(cf_proc_macros::HasTypeIntrospection)]
                        pub enum Enum {
                            $(
                                $variant(Ty::$variant),
                            )*
                        }

                        impl IsHistoricalType for Enum where
                            $( Ty::$variant: IsHistoricalType,)*
                        {
                            type GetCurrentType = $enum$(< $($T,)+ >)?;
                        }

                        mod where $( (Ty::$variant: cf_utilities::type_introspection::HasTypeIntrospection) )* {
                            //
                            // TypeInfo
                            //
                            // Implemented manually so that the variant indices in the type registry match
                            // the actual Encode/Decode discriminants (which skip empty variants and respect
                            // user-provided discriminant values). The `_phantom` variant is excluded from
                            // the type info since it's not a real variant.
                            impl scale_info::TypeInfo for Enum
                            where
                                Ty: 'static,
                                $( $($T: 'static,)+ )?
                                $(
                                    Ty::$variant: scale_info::TypeInfo + 'static,
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
                            impl codec::Encode for Enum
                                where $( Ty::$variant: codec::Encode,)*
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
                                    // very unfortunately we have to move the implementation into an inner module
                                    // because for some reason the syntax
                                    //
                                    //  let $variant: u8;
                                    //
                                    // and all of its variants breaks if there is a struct (!) in scope with the
                                    // name $variant.
                                    //
                                    // So we move into a module where no external symbols are in scope.
                                    mod inner {
                                        mod bounds {
                                            use super::super::*;
                                            $( $( pub trait $T = $( $TBound )?; )+ )?
                                        }
                                        pub fn do_encode<Ty: super::Types, __W: codec::Output + ?Sized, $( $( $T: bounds::$T, )+ )?>(
                                            this: &super::Enum,
                                            dest: &mut __W,
                                        )
                                        where $( Ty::$variant: cf_utilities::type_introspection::HasTypeIntrospection + codec::Encode, )*
                                        {
                                            let mut _disc: u8 = 0;
                                            $(
                                                let $variant: u8;
                                                if !<Ty::$variant as cf_utilities::type_introspection::HasTypeIntrospection>::is_empty_type() {
                                                    $( _disc = $variant_discriminant as u8; )?
                                                    $variant = _disc;
                                                    _disc += 1;
                                                } else {
                                                    $variant = 0; // dummy value, variant will never be encoded
                                                }
                                            )*

                                            match this {
                                                $(
                                                    super::Enum::$variant(val) => {
                                                        codec::Encode::encode_to(&$variant, dest);
                                                        codec::Encode::encode_to(val, dest);
                                                    }
                                                )*
                                                super::Enum::_phantom(never, _) => match *never {}
                                            }
                                        }
                                    }

                                    inner::do_encode(self, dest)
                                }
                            }

                            impl codec::Decode for Enum
                                where $( Ty::$variant: codec::Decode,)*
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

                            impl codec::DecodeWithMemTracking for Enum
                                where
                                    $( Ty::$variant: codec::DecodeWithMemTracking,)*
                                    Self: codec::Decode,
                            {}

                            impl codec::MaxEncodedLen for Enum
                                where
                                    $( Ty::$variant: codec::MaxEncodedLen,)*
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
                            impl proptest::arbitrary::Arbitrary for Enum
                            where
                                Ty: 'static,
                                $( $($T: 'static,)+ )?
                                $(
                                    Ty::$variant: proptest::arbitrary::Arbitrary + sp_std::fmt::Debug + 'static,
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
                        }
                    }
                }
            }

            pub type see_variant_changelogs = see_variant_changelogs_and_also<()>;
            pub struct see_variant_changelogs_and_also<M>(M);

            cf_proc_macros::generic_modules! {
                mod (To: HistoricalTypesAt<V>) (V: Version)
                {
                    pub trait VariantCustomMigration {
                        $(
                            type $variant: MaybeMigration<To::$variant, V> = DefaultMigration;
                        )+
                    }

                    impl VariantCustomMigration for () {}

                    impl<M1: VariantCustomMigration, M2: VariantCustomMigration> VariantCustomMigration for (M1, M2) {
                        $(
                            type $variant = (M1::$variant, M2::$variant);
                        )+
                    }

                    mod (M: VariantCustomMigration<To, V>)
                    {
                        mod variant_migrations {
                            use super::*;
                            $(
                                pub type $variant = <M::$variant as MaybeMigration<To::$variant, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$variant, V>>;
                            )*
                            pub type From = (
                                $(
                                    <variant_migrations::$variant as Migration<To::$variant, V>>::From,
                                )*
                            );

                            #[derive(cf_proc_macros::IntroElim)]
                            pub enum EnumForwardsError {
                                $(
                                    $variant(<$variant as Migration<To::$variant, V>>::ForwardsError),
                                )*
                            }

                            #[derive(cf_proc_macros::IntroElim)]
                            pub enum EnumBackwardsError {
                                $(
                                    $variant(<$variant as Migration<To::$variant, V>>::BackwardsError),
                                )*
                            }

                            impl cf_utilities::never::IsEmptyType for EnumForwardsError
                                where $(
                                    <$variant as Migration<To::$variant, V>>::ForwardsError: cf_utilities::never::IsEmptyType,
                                )*
                            {
                                fn as_never(&self) -> cf_utilities::never::Never {
                                    self.elim_ref(
                                        $(
                                            |err: &<$variant as Migration<To::$variant, V>>::ForwardsError| err.as_never(),
                                        )*
                                        |never, _phantom| never.as_never(),
                                    )
                                }
                            }

                            impl cf_utilities::never::IsEmptyType for EnumBackwardsError
                                where $(
                                    <$variant as Migration<To::$variant, V>>::BackwardsError: cf_utilities::never::IsEmptyType,
                                )*
                            {
                                fn as_never(&self) -> cf_utilities::never::Never {
                                    self.elim_ref(
                                        $(
                                            |err: &<$variant as Migration<To::$variant, V>>::BackwardsError| err.as_never(),
                                        )*
                                        |never, _phantom| never.as_never(),
                                    )
                                }
                            }

                            mod $( $( ($T $(: $TBound)?) )+ )? {
                                pub type EnumVariant<Target: Types> = Enum<$($($T,)+)? Target>;

                                impl Migration<Enum<$($($T,)+)? To>, V> for see_variant_changelogs_and_also<M> where
                                    EnumVariant<From>: IsHistoricalType,
                                    $enum<$($($T,)+)?>: HasChangelog
                                {
                                    type From = EnumVariant<From>;
                                    type ForwardsError = EnumForwardsError;
                                    type BackwardsError = EnumBackwardsError;

                                    fn try_forwards(x: Self::From) -> Result<EnumVariant<To>, Self::ForwardsError> {
                                        Ok(match x {
                                            $(
                                                Enum::$variant(val) => Enum::$variant(
                                                    $variant::try_forwards(val)
                                                        .map_err(EnumForwardsError::$variant)?
                                                ),
                                            )*
                                            Enum::_phantom(never, _) => Enum::_phantom(never, Default::default()),
                                        })
                                    }

                                    fn try_backwards(x: EnumVariant<To>) -> Result<Self::From, Self::BackwardsError> {
                                        Ok(match x {
                                            $(
                                                Enum::$variant(val) => Enum::$variant(
                                                    $variant::try_backwards(val)
                                                        .map_err(EnumBackwardsError::$variant)?
                                                ),
                                            )*
                                            Enum::_phantom(never, _) => Enum::_phantom(never, Default::default()),
                                        })
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ----------------- predefined migrations ------------------ //
            pub mod variant {
                $(
                    pub mod $variant {
                        use super::super::{OverrideMigrationWith, Version, HistoricalTypesAt, VariantCustomMigration, NewVariant};

                        #[derive(Debug)]
                        pub struct Added;
                        impl<V: Version, TargetVariantTypes: HistoricalTypesAt<V>>
                            VariantCustomMigration<TargetVariantTypes, V> for Added
                        {
                            type $variant = OverrideMigrationWith<NewVariant>;
                        }
                    }
                )*
            }


            // ----------------- connection with default enum ------------------ //

            cf_proc_macros::generic_modules! {
                mod $( $( ($T $(: $TBound)?) )+ )?
                {
                    pub type RealEnum = $enum $(< $($T,)+ >)?;

                    pub mod variants {
                        use super::*;
                        pub mod __impls {
                            use super::*;
                            $(
                                pub mod $variant {
                                    use super::*;
                                    $crate::generate_module! {
                                        pub struct variant_struct {
                                            $( $( pub $variant_tuple_entry: $variant_ty,)*)?
                                            $( $( pub $variant_field: $variant_field_ty,)*)?
                                        }
                                        mod variant_mod { #![migrations] }
                                    }
                                    impl HasChangelog for variant_struct
                                    where
                                        $( $( $variant_ty: HasChangelog, )* )?
                                        $( $( $variant_field_ty: HasChangelog, )* )?
                                    {
                                        type if_unspecified = variant_mod::see_field_changelogs;
                                    }
                                }
                            )*
                        }

                        $(
                            pub type $variant = __impls::$variant::variant_struct;

                            impl From<$variant> for RealEnum {
                                fn from(this: $variant) -> RealEnum {
                                    $enum::$variant
                                    $(
                                        {
                                            $( $variant_field: this.$variant_field, )*
                                        }
                                    )?
                                    $(
                                        (
                                            $( this.$variant_tuple_entry, )*
                                        )
                                    )?
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
                                $( $( $variant_ty: HasChangelog, )* )?
                                $( $( $variant_ty: HasGenericVariant<GenericType: IsHistoricalType>, )* )?
                                $( $( $variant_field_ty: HasChangelog, )* )?
                                $( $( $variant_field_ty: HasGenericVariant<GenericType: IsHistoricalType>, )* )?
                            )*
                            $enum<$($($T,)+)?>: HasChangelog,
                            Enum<$($($T,)+)? (
                                $(
                                    GetGenericVariant<variants::$variant>,
                                )*
                            )>: IsHistoricalType
                    {
                        type GenericType = Enum<$($($T,)+)? (
                            $(
                                GetGenericVariant<variants::$variant>,
                            )*
                        )>;
                        type MigrationFromGeneric = GlobalMigrationFromGeneric;
                    }


                    impl Migration<RealEnum, vCurrent> for GlobalMigrationFromGeneric
                    where
                            $(
                                $( $( $variant_ty: HasChangelog, )* )?
                                $( $( $variant_ty: HasGenericVariant<GenericType: IsHistoricalType>, )* )?
                                $( $( $variant_field_ty: HasChangelog, )* )?
                                $( $( $variant_field_ty: HasGenericVariant<GenericType: IsHistoricalType>, )* )?
                            )*
                            $enum<$($($T,)+)?>: HasChangelog,
                        Enum<$($($T,)+)? (
                            $(
                                GetGenericVariant<variants::$variant>,
                            )*
                        )>: IsHistoricalType
                    {
                        type From = Enum<$($($T,)+)? (
                            $(
                                GetGenericVariant<variants::$variant>,
                            )*
                        )>;
                        type ForwardsError = cf_utilities::never::Never;
                        type BackwardsError = cf_utilities::never::Never;

                        fn try_forwards(x: Self::From) -> Result<RealEnum, Self::ForwardsError> {
                            Ok(
                                match x {
                                    $(
                                        Enum::$variant(val) =>
                                            (<<variants::$variant as HasGenericVariant>::MigrationFromGeneric as Migration<variants::$variant, vCurrent>>::try_forwards(val)?).into(),
                                    )*
                                    Enum::_phantom(never, _) => match never {},
                                }
                            )
                        }

                        fn try_backwards(x: RealEnum) -> Result<Self::From, Self::BackwardsError> {
                            x.elim(
                                $(
                                    |$($($variant_tuple_entry: $variant_ty,)*)? $($($variant_field: $variant_field_ty,)*)?|
                                    <<variants::$variant as HasGenericVariant>::MigrationFromGeneric as Migration<variants::$variant, vCurrent>>::try_backwards(
                                        variants::$variant::intro(
                                            $( $( $variant_tuple_entry, )*)?
                                            $( $( $variant_field, )*)?
                                            Default::default(),
                                        )
                                    ).map(Enum::$variant),
                                )*
                            )
                        }
                    }

                }
            }
        }
    };
}
pub use generate_module;
