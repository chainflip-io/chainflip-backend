
#[macro_export]
macro_rules! generic_item {
    (
        mod $mod:ident {
            $(
                $field:ident,
            )*
        }
    ) => {
        pub mod $mod {
            #![allow(nonstandard_style)]
            #![allow(unused)]

            use crate::runtime_apis::transparent_rpc_generator::transformations::*;
            use crate::runtime_apis::transparent_rpc_generator::type_variants::*;
            use pallet_cf_elections::generic_tools::CommonTraits;


            pub trait Types: 'static {
                $(
                    type $field: CommonTraits;
                )*
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