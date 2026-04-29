


// ---------- definition of migrations ------------


pub trait Migration {
	type From;
	type To;
    fn forwards(x: Self::From) -> Self::To;
    fn backwards(x: Self::To) -> Self::From;
}

pub type Source<M: Migration> = M::From;
pub type Target<M: Migration> = M::To;

// -------- identity migration --------
pub struct IdentityMigration<X>(sp_std::marker::PhantomData<X>);

impl<X> Migration for IdentityMigration<X> {
	type From = X;
	type To = X;
    
    fn forwards(x: Self::From) -> Self::To {
        x
    }
    
    fn backwards(x: Self::To) -> Self::From {
        x
    }
}

// -------- composition of migrations --------
impl<A: Migration, B: Migration<From = A::To>> Migration for (A,B) {
    type From = A::From;
    type To = B::To;
    
    fn forwards(x: Self::From) -> Self::To {
        B::forwards(A::forwards(x))
    }
    
    fn backwards(x: Self::To) -> Self::From {
        A::backwards(B::backwards(x))
    }
}

// -------- typed migration ---------


pub trait HasVariant<V: VariantName> {
    type Get;
}

pub trait TypedMigration<From,To> {
    fn forwards(x: From) -> To;
    fn backwards(x: To) -> From;
}

pub struct FromTypedMigration<From, To, M: TypedMigration<From,To>>(From,To,M);
impl<From, To, M: TypedMigration<From,To>> Migration for FromTypedMigration<From,To,M> {
    type From = From;
    type To = To;
    fn forwards(x: Self::From) -> Self::To {
        M::forwards(x)
    }

    fn backwards(x: Self::To) -> Self::From {
        M::backwards(x)
    }
}


// -------- list of all migrations -----------

pub trait Migrations: Sized {
	type From_01_09_To_02_00: Migration<To = Source<Self::From_02_00_To_02_01>> =
		IdentityMigration<Source<Self::From_02_00_To_02_01>>;
	type From_02_00_To_02_01: Migration<To = Source<Self::From_02_01_To_02_02>> =
		IdentityMigration<Source<Self::From_02_01_To_02_02>>;
	type From_02_01_To_02_02: Migration<To = Self> = IdentityMigration<Self>;
}

macro_rules! declare_all_migrations {
    (from $version1:literal => $migration1:ident; from $version2:literal => $migration2:ident; $($rest:tt)*) => {
        impl<X: Migrations> HasMigrationFrom<RuntimeVersion<$version1>> for X {
            type GetMigration = (X::$migration1 , <X as HasMigrationFrom<RuntimeVersion<$version2>>>::GetMigration);
        }
        declare_all_migrations! {
            from $version2 => $migration2; $($rest)*
        }
    };
    (from $version1:literal => $migration1:ident;) => {
        impl<X: Migrations> HasMigrationFrom<RuntimeVersion<$version1>> for X {
            type GetMigration = X::$migration1;
        }
    };
}

declare_all_migrations! {
    from 01_09 => From_01_09_To_02_00;
    from 02_00 => From_02_00_To_02_01;
    from 02_01 => From_02_01_To_02_02;
}

// --------- definition of variants -----------

pub trait VariantName {}


pub trait HasMigrationFrom<V: VariantName> {
	type GetMigration: Migration<To = Self>;
}
pub type GetVariant<V: VariantName, X> = <X as HasMigrationFrom<V>>::GetMigration;


// ---------- concrete variants ------------

pub struct RuntimeVersion<const MAJOR_MINOR: usize>;
impl<const MAJOR_MINOR: usize> VariantName for RuntimeVersion<MAJOR_MINOR> {}


// impl<X: Migrations> HasMigrationFrom<RuntimeVersion<02_01>> for X {
//     type GetMigration = X::From_02_01_To_02_02;
// }
// impl<X: Migrations> HasMigrationFrom<RuntimeVersion<02_00>> for X {
//     type GetMigration = (X::From_02_00_To_02_01, X::From_02_01_To_02_02);
// }
// impl<X: Migrations> HasMigrationFrom<RuntimeVersion<1,9>> for X {
//     type GetMigration = ((X::From_01_09_To_02_00, X::From_02_00_To_02_01), X::From_02_01_To_02_02);
// }




pub struct V2_2;
impl VariantName for V2_2 {}
pub struct V2_1;
impl VariantName for V2_1 {}
pub struct V2_0;
impl VariantName for V2_0 {}
pub struct AtRpc;
impl VariantName for AtRpc {}

pub struct AtRuntime;
impl VariantName for AtRuntime {}




