

use cf_chains::evm;
use cf_primitives::{Asset, AssetAmount};
use cf_utilities::rpc::NumberOrHex;
use pallet_cf_swapping::AffiliateDetails;
use sp_runtime::AccountId32;


trait Num {
    type Prev: Num;
    type Split1: Num;
    type Split2: Num;
}
struct Zero;
impl Num for () {
    type Prev = ();
    type Split1 = ();
    type Split2 = ();
}
impl Num for Zero {
    type Prev = Zero;
    type Split1 = ();
    type Split2 = ();
}
struct One;
impl Num for One {
    type Prev = ();
    type Split1 = ();
    type Split2 = ();
}
struct Suc<N: Num>(N);
impl<N: Num> Num for Suc<N> {
    type Prev = N;
    type Split1 = ();
    type Split2 = ();
}
impl<N: Num, M: Num> Num for (N, M) {
    type Prev = ();
    type Split1 = N;
    type Split2 = M;
}

trait Filler<N: Num> {
    type Get;
    type Next: Filler<N::Prev>;
    type Split1: Filler<N::Split1>;
    type Split2: Filler<N::Split2>;
}

impl Num for ! {
    type Prev = ();
    type Split1 = ();
    type Split2 = ();
}


trait Container<N: Num> {
    type With<F: Filler<N>>;
    type InputHom<A: Filler<N>, B: Filler<N>>;
    fn map<A: Filler<N>, B: Filler<N>>(f: &Self::InputHom<A,B>, x: Self::With<A>) -> Self::With<B>;
}


impl Filler<()> for () {
    type Get = ();
    type Next = ();
    type Split1 = ();
    type Split2 = ();
}

impl<A> Filler<One> for A {
    type Get = A;
    type Next = ();
    type Split1 = ();
    type Split2 = ();
}

impl<M: Num,N: Num, A: Filler<M>, B: Filler<N>> Filler<(M,N)> for (A,B) {
    type Get = ();
    type Next = ();
    type Split1 = A;
    type Split2 = B;
}

/*
impl Container<One> for ! {
    type With<F: Filler<One>> = F::Get;

    type InputHom<A: Filler<Self::N>, B: Filler<Self::N>> = Box<dyn Fn(A::Get) -> B::Get>;
    
    fn map<A: Filler<Self::N>, B: Filler<Self::N>>(f: &Self::InputHom<A,B>, x: Self::With<A>) -> Self::With<B> {
        f(x)
    }
}

impl<F: Container> Container for Vec<F> {
    type N = F::N;
    type With<A: Filler<Self::N>> = Vec<F::With<A>>;
    
    type InputHom<X: Filler<Self::N>, Y: Filler<Self::N>> = F::InputHom<X,Y>;
    
    fn map<A: Filler<Self::N>, B: Filler<Self::N>>(f: &Self::InputHom<A,B>, x: Self::With<A>) -> Self::With<B> {
        x.into_iter().map(|elem| F::map(f, elem)).collect()
    }
}

impl<F: Container> Container for Option<F> {
    type N = F::N;
    type With<A: Filler<Self::N>> = Option<F::With<A>>;
    
    type InputHom<X: Filler<Self::N>, Y: Filler<Self::N>> = F::InputHom<X,Y>;
    
    fn map<A: Filler<Self::N>, B: Filler<Self::N>>(f: &Self::InputHom<A,B>, x: Self::With<A>) -> Self::With<B> {
        x.map(|elem| F::map(f, elem))
    }
}


impl<F: Container, G: Container> Container for (F, G) {
    type N = (F::N, G::N);
    
    type With<A: Filler<Self::N>> = (F::With<A::Split1>, G::With<A::Split2>);
    
    type InputHom<A: Filler<Self::N>, B: Filler<Self::N>> = (
        F::InputHom<A::Split1, B::Split1>,
        G::InputHom<A::Split2, B::Split2>,
    );
    
    fn map<A: Filler<Self::N>, B: Filler<Self::N>>(f: &Self::InputHom<A,B>, x: Self::With<A>) -> Self::With<B> {
        (
            F::map(&f.0, x.0),
            G::map(&f.1, x.1),
        )
    }
}

#[duplicate::duplicate_item(T; [Asset]; [AccountId32]; [u32]; [u8]; [u16])]
impl Container for T {
    type N = ();
    type With<F: Filler<Self::N>> = T;

    type InputHom<A: Filler<Self::N>, B: Filler<Self::N>> = ();

    fn map<A: Filler<Self::N>, B: Filler<Self::N>>(f: &Self::InputHom<A,B>, x: Self::With<A>) -> Self::With<B> {
        x
    }
}

 */




macro_rules! generic_struct {
    (
        mod $mod:ident {
            $(
                $field:ident,
            )*
        }
    ) => {
        mod $mod {
            #[allow(unused)]
            use super::*;

            pub trait Types {
                $(
                    type $field;
                )*
                type To<Other: Types>: Types<
                    $(
                        $field: Fn<(Self::$field,), Output = Other::$field>,
                    )*
                > = (
                    $(
                        Box<dyn Fn(Self::$field) -> Other::$field>,
                    )*
                );
            }
            pub type Hom<T: Types, S: Types> = All<T::To<S>>;
            pub trait Nums = Types<
                $(
                    $field: Num,
                )*
            >;
            pub trait Shape<N: Nums> = Types<
                $(
                    $field: Container<N::$field>,
                )*
            >;
            pub trait ShapeFiller<N: Nums> = Types<
                $(
                    $field: Filler<N::$field>,
                )*
            >;
            pub type FilledShape<N: Nums, S: Shape<N>, F: ShapeFiller<N>> = (
                $(
                    <S::$field as Container<N::$field>>::With<F::$field>,
                )*
            );
            pub type InputHom<N: Nums, S: Shape<N>, A: ShapeFiller<N>, B: ShapeFiller<N>> = (
                $(
                    <S::$field as Container<N::$field>>::InputHom<A::$field, B::$field>,
                )*
            );

            impl<
                $(
                    $field,
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

            pub struct All<T: Types> {
                $(
                    pub $field: T::$field,
                )*
            }
            impl<T: Types> All<T> {
                pub fn map<S: Types>(self, f: All<T::To<S>>) -> All<S> {
                    All {
                        $(
                            $field: (f.$field)(self.$field),
                        )*
                    }
                }
            }

        }
    }
}

macro_rules! generic_struct_instance {
    (
        struct $struct:ident<$parent:ident: $parent_mod:ident>: $mod:ident  {
            $(
                $field:ident: [$container_ty:ty] for $field_ty:ty ,
            )*
        }
    ) => {
        struct $struct<Parent: $parent_mod::Types> {
            _phantom: std::marker::PhantomData<Parent>
        }
        impl<$parent: $parent_mod::Types> $mod::Types for $struct<$parent> {
            $(
                type $field = $field_ty;
            )*
        }
    };
}

pub macro generic_struct3 {
    ($($token:tt)*) => {

    }
}

mod broker_info {
    use super::*;

    #[allow(nonstandard_style)]
    pub trait Types {
        type earned_fees;
        type btc_vault_deposit_address;
        type affiliates;
        type bond;
        type bound_fee_withdrawal_address;

        type To<Other: Types>: Types<
            earned_fees: Fn<(Self::earned_fees,), Output = Other::earned_fees>,
            btc_vault_deposit_address: Fn<(Self::btc_vault_deposit_address,), Output = Other::btc_vault_deposit_address>,
            affiliates: Fn<(Self::affiliates,), Output = Other::affiliates>,
            bond: Fn<(Self::bond,), Output = Other::bond>,
            bound_fee_withdrawal_address: Fn<(Self::bound_fee_withdrawal_address,), Output = Other::bound_fee_withdrawal_address>,
        >
        = (
            Box<dyn Fn(Self::earned_fees) -> Other::earned_fees>,
            Box<dyn Fn(Self::btc_vault_deposit_address) -> Other::btc_vault_deposit_address>,
            Box<dyn Fn(Self::affiliates) -> Other::affiliates>,
            Box<dyn Fn(Self::bond) -> Other::bond>,
            Box<dyn Fn(Self::bound_fee_withdrawal_address) -> Other::bound_fee_withdrawal_address>,
        );
    }

    #[allow(nonstandard_style)]
    impl<
        earned_fees,
        btc_vault_deposit_address,
        affiliates,
        bond,
        bound_fee_withdrawal_address,
    > Types for ( earned_fees, btc_vault_deposit_address, affiliates, bond, bound_fee_withdrawal_address,) {
        type earned_fees = earned_fees;
        type btc_vault_deposit_address = btc_vault_deposit_address;
        type affiliates = affiliates;
        type bond = bond;
        type bound_fee_withdrawal_address = bound_fee_withdrawal_address;
    }
    // struct For<L: Layer> {
    //     _phantom: std::marker::PhantomData<L>
    // }
    // impl<L: Layer> Types for For<L> {
    //     type earned_fees = Vec<(Asset, L::AssetAmount)>;
    //     type btc_vault_deposit_address = Option<L::BtcAddress>;
    //     type affiliates = Vec<(AccountId32, AffiliateDetails)>;
    //     type bond = L::AssetAmount;
    //     type bound_fee_withdrawal_address = Option<evm::Address>;
    // }
    // struct All<T: Types> {
    //     earned_fees: T::earned_fees,
	//     btc_vault_deposit_address: T::btc_vault_deposit_address,
	//     affiliates: T::affiliates,
	//     bond: T::bond,
	//     bound_fee_withdrawal_address: T::bound_fee_withdrawal_address,
    // }
    // impl<T: Types> All<T> {
    //     fn map<S: Types>(self, f: All<T::To<S>>) -> All<S> {
    //         All {
    //             earned_fees: (f.earned_fees)(self.earned_fees),
    //             btc_vault_deposit_address: (f.btc_vault_deposit_address)(self.btc_vault_deposit_address),
    //             affiliates: (f.affiliates)(self.affiliates),
    //             bond: (f.bond)(self.bond),
    //             bound_fee_withdrawal_address: (f.bound_fee_withdrawal_address)(self.bound_fee_withdrawal_address),
    //         }
    //     }
    // }
    // pub type Hom<T: Types, S: Types> = All<T::To<S>>;
}

generic_struct!{
    mod layer {
        AssetAmount, BtcAddress, AccountId,
    }
}

generic_struct!{
    mod broker_info2 {
        earned_fees,
        btc_vault_deposit_address,
        affiliates,
        bond,
        bound_fee_withdrawal_address,
    }
}

struct BrokerInfoShape;
impl broker_info2::Types for BrokerInfoShape {
    type earned_fees = Vec<(Asset, !)>;
    type btc_vault_deposit_address = Option<!>;
    type affiliates = Vec<(AccountId32, !)>;
    type bond = (!, !);
    type bound_fee_withdrawal_address = Option<!>;
}

impl<L: layer::Types> broker_info2::Types for L {
    type earned_fees = ((), L::AssetAmount, );
    type btc_vault_deposit_address = L::BtcAddress;
    type affiliates = ((), L::AccountId);
    type bond = (u8, u8,);
    type bound_fee_withdrawal_address = u16;
}

fn domap() -> layer::Hom<RuntimeLayer, RpcLayer> {
    layer::All {
        AssetAmount: todo!(),
        BtcAddress: todo!(),
        AccountId: todo!(),
    }
}

// fn mymap<K: layer::Types, L: layer::Types>(f: layer::Hom<K, L>) -> broker_info2::Hom<K,L>{
//     broker_info2::All {
//         earned_fees: (todo!(), todo!()),
//         btc_vault_deposit_address: todo!(),
//         affiliates: todo!(),
//         bond: todo!(),
//         bound_fee_withdrawal_address: todo!(),
//     }
// }

// type BrokerInfo<L: layer::Types> = broker_info2::FilledShape<BrokerInfoShape, L>;

// pub fn mymap(x: BrokerInfo<RuntimeLayer>) -> BrokerInfo<RpcLayer> {



// }


// generic_struct_instance!{
//     struct BrokerInfo2<L: layer>: broker_info2 {
//         earned_fees: [u8] for Vec<!>,
//         btc_vault_deposit_address: [Id] for L::BtcAddress,
//         affiliates: Vec<u16>,
//         bond: L::AssetAmount,
//         bound_fee_withdrawal_address: L::BtcAddress,
//     }
// }



pub struct RuntimeLayer;
pub struct RpcLayer;

impl layer::Types for RuntimeLayer {
    type AssetAmount = AssetAmount;
    type BtcAddress = u8;
    type AccountId = u32;
}

impl layer::Types for RpcLayer {
    type AssetAmount = NumberOrHex;
    type BtcAddress = u16;
    type AccountId = u16;
}

pub fn get_migration() -> layer::Hom<RuntimeLayer, RpcLayer> {
    layer::All {
        AssetAmount: Box::new(|a| NumberOrHex::Number(a as u64)),
        BtcAddress: Box::new(|a| a as u16),
        AccountId: todo!(),
    }
}



generic_struct3! {
    pub struct BrokerInfo<L: Layer> {
        pub earned_fees: L::AssetAmount in Vec<(Asset, !)>,
    }
}

// #[derive(Encode, Decode, TypeInfo, DefaultNoBound)]
// #[derive_n_functor]
// pub struct BrokerInfo<L: Layer> {
// 	pub earned_fees: Vec<(Asset, L::AssetAmount)>,
// 	pub btc_vault_deposit_address: Option<L::BtcAddress>,
// 	pub affiliates: Vec<(AccountId32, AffiliateDetails)>,
// 	pub bond: L::AssetAmount,
// 	pub bound_fee_withdrawal_address: Option<evm::Address>,
// }


// pub enum AccountInfoByRole {
//     Unregistered(),
//     Broker(BrokerInfo<>)
// }
