use cf_chains::evm;
use cf_primitives::{Asset, AssetAmount};
use cf_utilities::rpc::NumberOrHex;
use pallet_cf_swapping::AffiliateDetails;
use sp_runtime::AccountId32;

trait Test {
    type Get;
}

trait Proj<const S: &'static str> {
    type Get;
}

// struct Proj<const S: &'static str>;

// impl Test for Proj<"bla"> {
//     type Get = u8;
// }
// impl Test for Proj<"other"> {
//     type Get = u16;
// }
// impl<const S: &'static str> Test for Proj<S> {
//     default type Get = ();
// }

impl Proj<"bla"> for () {
    type Get = u8;
}
impl<const S: &'static str> Proj<S> for () {
    default type Get = ();
}

trait Bla {
    type X<const S: &'static str>: Proj<S>;
}

impl Bla for () {
    type X<const b: &'static str> = ();
}

fn myf<T: Bla>(x: T::X<"bla">) {

}
// fn otherf() {
//     myf::<()>(0u8);
// }


trait Cat {
    type Prev: Cat = ();
    type Split1: Cat = ();
    type Split2: Cat = ();
    type Hom<X: Object<Self>, Y: Object<Self>>: 'static = ();
}
struct Zero;
impl Cat for () {
    type Prev = ();
    type Split1 = ();
    type Split2 = ();
}
impl Cat for Zero {
    type Prev = Zero;
    type Split1 = ();
    type Split2 = ();
}
struct One;
impl Cat for One {
    type Prev = ();
    type Split1 = ();
    type Split2 = ();
    type Hom<X: Object<Self>, Y: Object<Self>> = Box<dyn Fn(X::Get) -> Y::Get>;
}
struct Suc<N: Cat>(N);
impl<N: Cat> Cat for Suc<N> {
    type Prev = N;
    type Split1 = ();
    type Split2 = ();
}
impl<N: Cat, M: Cat> Cat for (N, M) {
    type Prev = ();
    type Split1 = N;
    type Split2 = M;
    type Hom<X: Object<Self>, Y: Object<Self>> = (N::Hom<X::Split1, Y::Split1>, M::Hom<X::Split2, Y::Split2>);
}

trait Object<N: Cat>: 'static {
    type Get: 'static;
    type Next: Object<N::Prev>;
    type Split1: Object<N::Split1>;
    type Split2: Object<N::Split2>;
}

struct Ty;
impl Cat for Ty {
    type Prev = ();
    type Split1 = ();
    type Split2 = ();
    type Hom<X: Object<Self>, Y: Object<Self>> = Box<dyn Fn(X::Get) -> Y::Get>;
}

impl<A: 'static> Object<Ty> for A {
    type Get = A;
    type Next = ();
    type Split1 = ();
    type Split2 = ();
}


type HomOf<C: Cat, X: Object<C>, Y: Object<C>> = C::Hom<X,Y>;

trait Container<Output: Cat> {
    type Input: Cat + 'static;
    type With<X: Object<Self::Input>>: Object<Output>;
    fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> Output::Hom<Self::With<X>, Self::With<Y>>;
}


impl Object<()> for () {
    type Get = ();
    type Next = ();
    type Split1 = ();
    type Split2 = ();
}

impl<M: Cat,N: Cat, A: Object<M>, B: Object<N>> Object<(M,N)> for (A,B) {
    type Get = ();
    type Next = ();
    type Split1 = A;
    type Split2 = B;
}

impl Container<Ty> for ! {
    type Input = Ty;

    type With<X: Object<Ty>> = X;
    
    fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
        Box::new(|x| f(x))
    }
}

impl<F: Container<Ty>> Container<Ty> for Vec<F> {
    type Input = F::Input;
    type With<X: Object<F::Input>> = Vec<<F::With<X> as Object<Ty>>::Get>;

    fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
        Box::new(|xs| xs.into_iter().map(F::map(f)).collect())
    }
}

impl<F: Container<Ty>> Container<Ty> for Option<F> {
    type Input = F::Input;
    type With<X: Object<F::Input>> = Option<<F::With<X> as Object<Ty>>::Get>;

    fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
        Box::new(|xs| xs.map(F::map(f)))
    }
}

// impl<C: Cat, D: Cat, F: Container<C>, G: Container<D>> Container<(C, D)> for (F, G) {
//     type Input = (F::Input, G::Input);
//     type With<X: Object<Self::Input>> = (F::With<X::Split1>, G::With<X::Split2>);

//     fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <(C, D) as Cat>::Hom<Self::With<X>, Self::With<Y>> {
//         (F::map(f.0), G::map(f.1))
//     }
// }

impl<F: Container<Ty>, G: Container<Ty>> Container<Ty> for (F, G) {
    type Input = (F::Input, G::Input);
    type With<X: Object<Self::Input>> = (<F::With<X::Split1> as Object<Ty>>::Get, <G::With<X::Split2> as Object<Ty>>::Get);

    fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> HomOf<Ty,Self::With<X>, Self::With<Y>> {
        Box::new(|(x,y)| (F::map(&f.0)(x), G::map(&f.1)(y)))
    }
}



#[duplicate::duplicate_item(T; [Asset]; [AccountId32]; [u32]; [u8]; [u16])]
impl Container<Ty> for T {
    type Input = ();

    type With<X: Object<Self::Input>> = T;

    fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(_f: &'static HomOf<Self::Input,X,Y>) -> HomOf<Ty,Self::With<X>, Self::With<Y>> {
        Box::new(|x| x)
    }
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


 /* 


macro_rules! generic_struct {
    (
        mod $mod:ident {
            $(
                $field:ident,
            )*
        }
    ) => {
        mod $mod {
            #![allow(nonstandard_style)]
            #[allow(unused)]
            use super::*;

            pub trait Types: 'static {
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
            // pub type Hom<T: Cats, S: Cats> = All<T::To<S>>;

            pub struct ConstTypes<A>(A);
            impl<A: 'static> Types for ConstTypes<A> {
                $(
                    type $field = A;
                )*
            }

            // The (fibered) category created by the struct
            pub struct FiberedCat<T: Types>(T);
            pub trait Cats = Types<
                $(
                    $field: super::Cat,
                )*
            >;
            impl<cats: Cats> Cat for FiberedCat<cats> {
                type Prev = ();
                type Split1 = ();
                type Split2 = ();
                // type Hom<X: Object<Self>, Y: Object<Self>> = All<(
                //     $(
                //         HomOf<cats::$field, <ObjectTypes<cats, X> as Types>::$field, <ObjectTypes<cats, Y> as Types>::$field>,
                //     )*
                // )>;
            }
            pub struct ObjectTypes<cats: Cats, X: Object<FiberedCat<cats>>>(std::marker::PhantomData<(cats, X)>);
            impl<cats: Cats, X: Object<FiberedCat<cats>>> Types for ObjectTypes<cats, X> {
                $(
                    type $field = (); // TODO
                )*
            }
            pub trait Objects<cats: Cats> = Types<
                $(
                    $field: Object<cats::$field>,
                )*
            >;
            impl<cats: Cats, Xs: Objects<cats>> Object<FiberedCat<cats>> for All<Xs> {
                type Get = ();
                type Next = ();
                type Split1 = ();
                type Split2 = ();
            }

            // The functor created by a set of functors
            pub trait Containers<C: Cats> = Types<
                $(
                    $field: super::Container<C::$field>,
                )*
            >;
            pub type ContainersInputs<cats: Cats, C: Containers<cats>> = (
                $(
                    <C::$field as Container<cats::$field>>::Input,
                )*
            );

            pub struct AllContainers<cats: Cats, C: Containers<cats>>(pub std::marker::PhantomData<(cats, C)>);

            impl<cats: Cats, C: Containers<cats>> Container<FiberedCat<cats>> for AllContainers<cats, C> {
                type Input = FiberedCat<ContainersInputs<cats, C>>;

                type With<X: Object<Self::Input>> = (); // All<(
                //     $(
                //         <C::$field as Container<cats::$field>>::With<<ObjectTypes<ContainersInputs<cats, C>, X> as Types>::$field>,
                //     )*
                // )>;

                fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <FiberedCat<cats> as Cat>::Hom<Self::With<X>, Self::With<Y>> {
                    todo!()
                    // All {
                    //     $(
                    //         $field: <C::$field as Container<cats::$field>>::map(f.$field),
                    //     )*
                    // }
                }
            }


            // pub trait Shape<N: Nums> = Types<
            //     $(
            //         $field: Container<N::$field>,
            //     )*
            // >;
            // pub trait ShapeFiller<N: Nums> = Types<
            //     $(
            //         $field: Filler<N::$field>,
            //     )*
            // >;
            // pub type FilledShape<N: Nums, S: Shape<N>, F: ShapeFiller<N>> = (
            //     $(
            //         <S::$field as Container<N::$field>>::With<F::$field>,
            //     )*
            // );
            // pub type InputHom<N: Nums, S: Shape<N>, A: ShapeFiller<N>, B: ShapeFiller<N>> = (
            //     $(
            //         <S::$field as Container<N::$field>>::InputHom<A::$field, B::$field>,
            //     )*
            // );

            impl<
                $(
                    $field: 'static,
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

// I need mapping with an arbitrary functor of type (Ty,Ty) => Ty
// 
// That takes (Cat<T>, Cat<T>) => Cat<T>
//
// We should be able to merge two different types according to a Ty Functor
//
// aka a fiberwise application of functors?
//
// If we have Object<C> 



// impl<Cs: broker_info2::Cats, Fs: broker_info2::Containers<Cs>> Container<broker_info2::FiberedCat<Cs>> for Fs {
//     type Input = ();

//     type With<X: Object<Self::Input>> = ();

//     fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <broker_info2::FiberedCat<Cs> as Cat>::Hom<Self::With<X>, Self::With<Y>> {
//         todo!()
//     }
// }

// #[allow(nonstandard_style)]
// impl<cats: broker_info2::Cats> Container<Ty> for broker_info2::All<cats> {
//     type Input = broker_info2::FiberedCat<cats>;

//     type With<X: Object<Self::Input>> = broker_info2::All<broker_info2::ObjectTypes<cats, X>>;

//     fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
//         todo!()
//     }
// }


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

// now from a Hom<layer::Cat,L,K> I can get a brokerinfo::Types

fn domap() -> HomOf<ShapeCat, RuntimeLayer, RpcLayer> {
    layer::All {
        AssetAmount: todo!(),
        BtcAddress: todo!(),
        AccountId: todo!(),
    }
}

type ShapeCat = <broker_info2::AllContainers<broker_info2::ConstTypes<Ty>, BrokerInfoShape> as Container<broker_info2::FiberedCat<broker_info2::ConstTypes<Ty>>>>::Input;


fn accept_functor<T: Container<broker_info2::FiberedCat<broker_info2::ConstTypes<Ty>>>, X: Object<T::Input>, Y: Object<T::Input>>(x: T, f: HomOf<T::Input,X,Y>) {

}

fn myresult() {
    accept_functor(broker_info2::AllContainers::<_, BrokerInfoShape>(Default::default()), domap());

    // let x: broker_info2::Hom<RuntimeLayer, RpcLayer> = 
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
*/