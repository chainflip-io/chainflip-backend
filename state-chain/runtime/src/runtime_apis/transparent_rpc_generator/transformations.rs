
#[macro_export]
macro_rules! generic_item {
    (
        struct $struct:ident$(<$T:ident: $TBound:path>)? {
            $(
                $field:ident: $field_ty:ty,
            )*
        }
        mod $mod:ident;
    ) => {
        pub struct $struct$(<$T: $TBound>)? {
            $(
                $field: $field_ty,
            )*
        }

        pub mod $mod {
            #![allow(nonstandard_style)]
            #![allow(unused)]

            use crate::runtime_apis::transparent_rpc_generator::transformations::*;
            use crate::runtime_apis::transparent_rpc_generator::type_variants::*;
            use pallet_cf_elections::generic_tools::CommonTraits;
            use super::*;


            pub trait Types: 'static {
                $(
                    type $field: CommonTraits;
                )*
            }

            pub struct DefaultTypes$(<$T: $TBound>)?(T);

            impl$(<$T: $TBound>)? Types for DefaultTypes$(<$T>)? {
                $(
                    type $field = $field_ty;
                )*
            }

            impl$(<$T: $TBound>)? From<Struct<DefaultTypes $(<$T>)? >> for $struct $(<$T>)? {
                fn from(x: Struct<DefaultTypes $(<$T>)? >) -> $struct $(<$T>)? {
                    $struct {
                        $(
                            $field: x.$field,
                        )*
                    }
                }
            }

            impl$(<$T: $TBound>)? Into<Struct<DefaultTypes $(<$T>)? >> for $struct $(<$T>)? {
                fn into(self) -> Struct<DefaultTypes $(<$T>)? > {
                    Struct {
                        $(
                            $field: self.$field,
                        )*
                    }
                }
            }

            // impl Types for () {
            //     $(
            //         type $field = $field_ty;
            //     )*
            // }

            // type Tuple<T: Types> = (
            //         $(
            //             T::$field,
            //         )*
            // );

            impl<
                $(
                    $field: 'static + CommonTraits,
                )*
            > Types for (
                $(
                    $field,
                )*
            ) {
                $(
                    type $field = $field;
                )*
            }

            cf_utilities::derive_common_traits_no_bounds! {
                #[derive(scale_info::TypeInfo)]
                #[scale_info(skip_type_params(T))]
                pub struct Struct<T: Types> {
                    $(
                        pub $field: T::$field,
                    )*
                }
            }

            // --------- Migrations -------------
            impl<
                T: Types, S: Types, M:
                $(
                  TypedMigration<T::$field, S::$field> +
                )*
            > TypedMigration<Struct<T>,Struct<S>> for M {

                fn forwards(x: Struct<T>) -> Struct<S> {
                    Struct {
                        $(
                            $field: M::forwards(x.$field),
                        )*
                    }
                }

                fn backwards(x: Struct<S>) -> Struct<T> {
                    Struct {
                        $(
                            $field: M::backwards(x.$field),
                        )*
                    }
                }

            }
            // pub struct MigrateFields
            // impl<
            //     X: Mig
            // >
        }
    }
}
pub use generic_item;