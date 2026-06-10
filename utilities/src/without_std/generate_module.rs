#[macro_export]
macro_rules! generate_module {
    (
		$(#[$($Attributes:tt)*])*
        $vis:vis struct $struct:ident$(<$T:ident $(: $TBound:path)?>)? {
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
        $vis struct $struct$(<$T $(: $TBound)?>)? {
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
            pub trait HistoricalTypesAt<V: VariantName> = Types<
                $(
                    $field: IsHistoricalTypeAt<V>,
                )*
            >;

            impl< $( $field,)* > Types for ( $($field,)* ) {
                $(
                    type $field = $field;
                )*
            }

            pub trait CustomMigration<
                To: HistoricalTypesAt<V>,
                V: VariantName,
            > {
                $(
                    type $field: MaybeMigration<To::$field, V> = DefaultMigration;
                    // type $field: MaybeMigration<To::$field, V> = GetMigrationToHistoricalType<To::$field, V>;
                )+
            }

            type ResolveCustomMigration<To: HistoricalTypesAt<V>, V: VariantName, M: CustomMigration<To, V>> = (
                $(
                    <M::$field as MaybeMigration<To::$field, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$field, V>>,
                )+
            );

            impl <
                To: HistoricalTypesAt<V>,
                V: VariantName
            > CustomMigration<To, V> for () {}

            impl<To: HistoricalTypesAt<V>, V: VariantName, M1: CustomMigration<To, V>, M2: CustomMigration<To, V>>
            CustomMigration<To,V>
            for (M1, M2)
            {
                $(
                    type $field = (M1::$field, M2::$field);
                )+
            }

            /// This is purely used for backwards compatibility with older runtimes, and won't be exposed on the
            /// rpc layer. So there's intentionally no Serialize/Deserialize implementation
            #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Encode, Decode, codec::DecodeWithMemTracking, TypeInfo, codec::MaxEncodedLen, Default)]
            #[cfg_attr(any(feature = "proptest", test), derive(proptest_derive::Arbitrary))]
            pub struct Struct<Ty: Types, $( $T $(: $TBound)?, )? > {
                $(
                    pub $field: Ty::$field,
                )+
                pub _phantom: sp_std::marker::PhantomData<($($T,)?)>,
            }

            impl<$( $T $(: $TBound)?, )? Ty: Types<$($field: IsHistoricalType,)*>> IsHistoricalType for Struct<Ty, $($T)?>
            where $struct$(<$T>)?: Migrations
            {
                type GetCurrentType = $struct$(<$T>)?;
            }


            pub struct MigrateFields<M = ()>(M);

            impl<M: CustomMigration<To, V>, $( $T $(: $TBound)?, )? To: HistoricalTypesAt<V>, V: VariantName> Migration<Struct<To, $($T)?>, V> for MigrateFields<M>
            where
                Struct<
                    // ResolveCustomMigration<To, V, M>
                    // $(
                    //     <M::$field as MaybeMigration<To::$field, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$field, V>>,
                    // )+
                    (
                    $(
                        <
                            <M::$field as MaybeMigration<To::$field, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$field, V>>
                            as Migration<To::$field, V>
                        >::From,
                    )+
                    )
                , $($T)?
                >: IsHistoricalType
            {
                type From = Struct< (
                    $(
                        <
                            <M::$field as MaybeMigration<To::$field, V>>::GetWithDefault<GetMigrationToHistoricalType<To::$field, V>>
                            as Migration<To::$field, V>
                        >::From,
                    )+
                ), $($T)?
                >;

                // Struct< ($( <M::$field as Migration<To::$field, V>>::From,)+), $($T)? >;

                fn forwards(x: Self::From) -> Struct<To, $($T)?> {
                    Struct {
                        $(
                            $field: <ResolveCustomMigration::<To, V, M> as Types>::$field::forwards(x.$field),
                        )+
                        _phantom: Default::default(),
                    }
                }

                fn backwards(x: Struct<To, $($T)?>) -> Self::From {
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
                        use super::super::{OverrideMigrationWith, VariantName, HistoricalTypesAt, CustomMigration, NewFieldWithDefault};

                        pub struct Added;
                        impl<V: VariantName, TargetFieldsTypes: HistoricalTypesAt<V, $field: Default>>
                            CustomMigration<TargetFieldsTypes, V> for Added
                        {
                            type $field = OverrideMigrationWith<NewFieldWithDefault>;
                        }
                    }
                )+
            }

            // ----------------- connection with default struct ------------------ //

            pub struct DefaultTypes$(<$T $(: $TBound)?>)?($($T)?);

            impl$(<$T $(: $TBound)?>)? Types for DefaultTypes$(<$T>)? {
                $(
                    type $field = $field_ty;
                )*
            }

            impl $(< $T $(: $TBound)?, >)? HasGenericVariant for $struct $(<$T>)?
            where $( $field_ty: HasGenericVariant,)*
                Struct<(
                    $(
                        GetGenericVariant<$field_ty>,
                    )*
                ), $($T)?>: IsHistoricalType
            {
                type GenericType = Struct<(
                    $(
                        GetGenericVariant<$field_ty>,
                    )*
                ), $($T)?>;
                type MigrationFromGeneric = GlobalMigrationFromGeneric;
            }

            impl $(< $T $(: $TBound)? >)? Migration<$struct $(<$T>)?, vCurrent> for GlobalMigrationFromGeneric
            where $( $field_ty: HasGenericVariant,)*
                Struct<(
                    $(
                        GetGenericVariant<$field_ty>,
                    )*
                ), $($T)?>: IsHistoricalType
            {
                type From = Struct<(
                    $(
                        GetGenericVariant<$field_ty>,
                    )*
                ), $($T)?>;

                fn forwards(x: Self::From) -> $struct $(<$T>)? {
                    $struct {
                        $(
                            $field: <<$field_ty as HasGenericVariant>::MigrationFromGeneric as Migration<$field_ty, vCurrent>>::forwards(x.$field),
                        )*
                    }
                }

                fn backwards(x: $struct $(<$T>)?) -> Self::From {
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
