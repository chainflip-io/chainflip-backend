// --------- primitives --------

use sp_core::crypto::{self, AccountId32};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use crate::migrations::{
	basics::{GetGenericVariant, IdentityMigration, Migration, VariantName},
	HasGenericVariant, IsHistoricalType, Migrations, OrdMigrations,
};

// ----------- identity migrations -------------

macro_rules! impl_identity_migrations {
	($($ty:ty, )*) => {

        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl IsHistoricalType for Type {
            type GetCurrentType = Self;
        }
        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl HasGenericVariant for Type {
            type GenericType = Type;
            type MigrationFromGeneric = IdentityMigration;
        }
        #[duplicate::duplicate_item(Type; $( [ $ty ] );* )]
        impl Migrations for Type {
            type DefaultMigration = IdentityMigration;
        }

    };
}

impl_identity_migrations! {(), u8, u128, sp_arithmetic::Permill, }

// ----------- wrapped types -------------

pub struct WrapMigration;

macro_rules! impl_identity_migrations_with_wrapper {
	(
        $(#[$meta:meta])*
        struct $Wrapper:ident ( $Ty:ty ) where $(|$var:ident: $Inner:ty| $ctr:expr)?;
    ) => {
		#[derive(
			codec::Encode,
			codec::Decode,
			scale_info::TypeInfo,
			serde::Serialize,
			serde::Deserialize,
			Clone,
			Debug,
		)]
        $(#[$meta])*
		pub struct $Wrapper(pub $Ty);

		impl Migration<$Ty, crate::migrations::vCurrent> for WrapMigration {
			type From = $Wrapper;

			fn forwards(x: Self::From) -> $Ty {
				x.0
			}

			fn backwards(x: $Ty) -> Self::From {
				$Wrapper(x)
			}
		}

		impl IsHistoricalType for $Wrapper {
			type GetCurrentType = $Ty;
		}
		impl HasGenericVariant for $Ty {
            type GenericType = $Wrapper;
			type MigrationFromGeneric = WrapMigration;
		}
		impl Migrations for $Ty {
			type DefaultMigration = IdentityMigration;
		}

        $(
            #[cfg(feature = "proptest")]
            impl proptest::arbitrary::Arbitrary for $Wrapper {
                type Parameters = <$Inner as proptest::prelude::Arbitrary>::Parameters;

                fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
                    use proptest::prelude::*;
                    <$Inner as Arbitrary>::arbitrary_with(args)
                        .prop_map(|$var| $Wrapper($ctr))
                }

                type Strategy = impl proptest::strategy::Strategy<Value = Self>;
            }
        )?
	};
}

impl_identity_migrations_with_wrapper! {
	struct WrappedAccountId32(sp_core::crypto::AccountId32) where |x: [u8; 32]| AccountId32::new(x);
}

impl_identity_migrations_with_wrapper! {
	#[derive(PartialOrd, PartialEq, Eq, Ord)]
	struct WrappedH160(sp_core::H160) where |x: [u8; 20]| x.into();
}

// ----------- containers -------------

pub struct MapMigration<X>(X);
impl<A, V: VariantName, M: Migration<A, V>> Migration<Option<A>, V> for MapMigration<M> {
	type From = Option<M::From>;

	fn forwards(x: Self::From) -> Option<A> {
		x.map(M::forwards)
	}

	fn backwards(x: Option<A>) -> Self::From {
		x.map(M::backwards)
	}
}

impl<X: Migrations> Migrations for Option<X> {
	type DefaultMigration = MapMigration<X::DefaultMigration>;

	// these have to be specified as otherwise the above
	// default migration doesn't go through. Because rust
	// is forced to work with arbitrary implementations for these,
	// and so can't prove that the historical types are actually
	// all of the shape `Option<...>`.
	type MigrationTo0200 = MapMigration<X::MigrationTo0200>;
	type MigrationTo0201 = MapMigration<X::MigrationTo0201>;
	type MigrationTo0202 = MapMigration<X::MigrationTo0202>;
}
impl<X: HasGenericVariant> HasGenericVariant for Option<X> {
	type GenericType = Option<X::GenericType>;
	type MigrationFromGeneric = MapMigration<X::MigrationFromGeneric>;
}
impl<X: IsHistoricalType> IsHistoricalType for Option<X> {
	type GetCurrentType = Option<X::GetCurrentType>;
}

impl<A, V: VariantName, M: Migration<A, V>> Migration<Vec<A>, V> for MapMigration<M> {
	type From = Vec<M::From>;

	fn forwards(x: Self::From) -> Vec<A> {
		x.into_iter().map(M::forwards).collect()
	}

	fn backwards(x: Vec<A>) -> Self::From {
		x.into_iter().map(M::backwards).collect()
	}
}

impl<X: Migrations> Migrations for Vec<X> {
	type DefaultMigration = MapMigration<X::DefaultMigration>;

	// these have to be specified as otherwise the above
	// default migration doesn't go through. Because rust
	// is forced to work with arbitrary implementations for these,
	// and so can't prove that the historical types are actually
	// all of the shape `Option<...>`.
	type MigrationTo0200 = MapMigration<X::MigrationTo0200>;
	type MigrationTo0201 = MapMigration<X::MigrationTo0201>;
	type MigrationTo0202 = MapMigration<X::MigrationTo0202>;
}
impl<X: HasGenericVariant> HasGenericVariant for Vec<X> {
	type GenericType = Vec<X::GenericType>;
	type MigrationFromGeneric = MapMigration<X::MigrationFromGeneric>;
}
impl<X: IsHistoricalType> IsHistoricalType for Vec<X> {
	type GetCurrentType = Vec<X::GetCurrentType>;
}

// btreemap

impl<
		A: Ord,
		B,
		V: VariantName,
		M1: Migration<A, V, From: IsHistoricalType<GetCurrentType: OrdMigrations + Ord> + Ord>,
		M2: Migration<B, V>,
	> Migration<BTreeMap<A, B>, V> for MapMigration<(M1, M2)>
{
	type From = BTreeMap<M1::From, M2::From>;

	fn forwards(x: Self::From) -> BTreeMap<A, B> {
		x.into_iter().map(|(a, b)| (M1::forwards(a), M2::forwards(b))).collect()
	}

	fn backwards(x: BTreeMap<A, B>) -> Self::From {
		x.into_iter().map(|(a, b)| (M1::backwards(a), M2::backwards(b))).collect()
	}
}

impl<A: OrdMigrations + Ord, B: Migrations> Migrations for BTreeMap<A, B> {
	type DefaultMigration = MapMigration<(A::DefaultMigration, B::DefaultMigration)>;

	// these have to be specified as otherwise the above
	// default migration doesn't go through. Because rust
	// is forced to work with arbitrary implementations for these,
	// and so can't prove that the historical types are actually
	// all of the shape `Option<...>`.
	type MigrationTo0200 = MapMigration<(A::MigrationTo0200, B::MigrationTo0200)>;
	type MigrationTo0201 = MapMigration<(A::MigrationTo0201, B::MigrationTo0201)>;
	type MigrationTo0202 = MapMigration<(A::MigrationTo0202, B::MigrationTo0202)>;
}
impl<A: HasGenericVariant + Ord, B: HasGenericVariant> HasGenericVariant for BTreeMap<A, B>
where
	A: HasGenericVariant<GenericType: Ord + IsHistoricalTypeOrd>,
{
	type GenericType = BTreeMap<A::GenericType, B::GenericType>;
	type MigrationFromGeneric = MapMigration<(A::MigrationFromGeneric, B::MigrationFromGeneric)>;
}
impl<A: IsHistoricalType<GetCurrentType: OrdMigrations + Ord>, B: IsHistoricalType> IsHistoricalType
	for BTreeMap<A, B>
{
	type GetCurrentType = BTreeMap<A::GetCurrentType, B::GetCurrentType>;
}

trait IsHistoricalTypeOrd = IsHistoricalType<GetCurrentType: OrdMigrations + Ord>;
