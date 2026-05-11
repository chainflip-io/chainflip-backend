// --------- primitives --------

use crate::migrations::{
	basics::IdentityMigration, HasGenericVariant, IsHistoricalType, Migrations,
};

macro_rules! impl_identity_migrations {
	($($ty:ty, )*) => {

        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl IsHistoricalType for Type {
            type GetCurrentType = Self;
        }
        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl HasGenericVariant for Type {
            type MigrationFromGeneric = IdentityMigration;
        }
        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl Migrations for Type {
            type DefaultMigration = IdentityMigration;
        }

    };
}

impl_identity_migrations! {(), u8, u128, sp_arithmetic::Permill, }
