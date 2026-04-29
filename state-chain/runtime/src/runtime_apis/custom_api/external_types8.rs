
// use cf_primitives::{Asset, AssetAmount};
// use cf_utilities::rpc::NumberOrHex;
// use sp_runtime::AccountId32;
use sp_std::boxed::Box;

// ------------------ categories & objects --------------------------

trait Cat: 'static + Sized {
    // sub categories
    type Get0: Cat = ();
    type Get1: Cat = ();
    type Get2: Cat = ();
    type Get3: Cat = ();
    type Get4: Cat = ();
    type Get5: Cat = ();
    type Get6: Cat = ();
    type Get7: Cat = ();
    type Get8: Cat = ();
    type Get9: Cat = ();

    // default object
    type DefaultObj: Object<Self>;

    type Hom<X: Object<Self>, Y: Object<Self>>: 'static = ();
}
type HomOf<C: Cat, X: Object<C>, Y: Object<C>> = C::Hom<X,Y>;

impl Cat for () {
    type DefaultObj = ();
}

trait Object<C: Cat>: 'static {
    type Ty: 'static;
    type Next0: Object<C::Get0> = <C::Get0 as Cat>::DefaultObj;
    type Next1: Object<C::Get1> = <C::Get1 as Cat>::DefaultObj;
    type Next2: Object<C::Get2> = <C::Get2 as Cat>::DefaultObj;
    type Next3: Object<C::Get3> = <C::Get3 as Cat>::DefaultObj;
    type Next4: Object<C::Get4> = <C::Get4 as Cat>::DefaultObj;
    type Next5: Object<C::Get5> = <C::Get5 as Cat>::DefaultObj;
    type Next6: Object<C::Get6> = <C::Get6 as Cat>::DefaultObj;
    type Next7: Object<C::Get7> = <C::Get7 as Cat>::DefaultObj;
    type Next8: Object<C::Get8> = <C::Get8 as Cat>::DefaultObj;
    type Next9: Object<C::Get9> = <C::Get9 as Cat>::DefaultObj;
}

impl Object<()> for () {
    type Ty = ();
}

// ------------------ type category --------------------------

struct Ty;

impl Cat for Ty {
    type Hom<X: Object<Self>, Y: Object<Self>> = Box<dyn Fn(X::Ty) -> Y::Ty>;
    type DefaultObj = ();
}
impl<A: 'static> Object<Ty> for A {
    type Ty = Self;
}

// ------------------ tuple categories --------------------------

macro_rules! implement_tuple_cat {
    ($($var_accessor:ident = $var:ident: $accessor:ident = $name:ident, )*) => {
        impl<
            $(
                $name: Cat,
            )*
        > Cat for (
            $(
                $name,
            )*
        ) {
            $(
                type $accessor = $name;
            )*
            type DefaultObj = (
                $(
                    $name::DefaultObj,
                )*
            );
            type Hom<X: Object<Self>, Y: Object<Self>> = (
                $(
                    HomOf<$name, X::$var_accessor, Y::$var_accessor>,
                )*
            );
        }
        impl<
            $(
                $name: Cat,
            )*
            $(
                $var: Object<$name>,
            )*
        > Object<($($name, )*)> for ($($var,)*) {
            type Ty = ();
            $(
                type $var_accessor = $var;
            )*
        }
    };
}

implement_tuple_cat!(
    Next0 = X0: Get0 = C0,
);
implement_tuple_cat!(
    Next0 = X0: Get0 = C0,
    Next1 = X1: Get1 = C1,
);
implement_tuple_cat!(
    Next0 = X0: Get0 = C0,
    Next1 = X1: Get1 = C1,
    Next2 = X2: Get2 = C2,
);
implement_tuple_cat!(
    Next0 = X0: Get0 = C0,
    Next1 = X1: Get1 = C1,
    Next2 = X2: Get2 = C2,
    Next3 = X3: Get3 = C3,
);
implement_tuple_cat!(
    Next0 = X0: Get0 = C0,
    Next1 = X1: Get1 = C1,
    Next2 = X2: Get2 = C2,
    Next3 = X3: Get3 = C3,
    Next4 = X4: Get4 = C4,
);

// // ------------------ wrapper category --------------------------

trait WrappedCategory: 'static {
    type Unwrap: Cat;
}
// struct Wrapped<C: WrappedCategory>(C);
// impl<C: WrappedCategory> Cat for Wrapped<C> {
//     type Get0 = C::Unwrap;
//     type DefaultObj;
//     type Hom<X: Object<Self>, Y: Object<Self>> = ();
// }
// impl<C: WrappedCategory> Object for Wrapped<C> {
//     type Ty = ();
//     type Next0 = <C::Get0 as Cat>::DefaultObj;
// }


// ------------------ containers --------------------------

trait Container<Output: Cat> {
    type Input: Cat + 'static;
    type With<X: Object<Self::Input>>: Object<Output>;
    fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> Output::Hom<Self::With<X>, Self::With<Y>>;
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
    type With<X: Object<F::Input>> = Vec<<F::With<X> as Object<Ty>>::Ty>;

    fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
        Box::new(|xs: Vec<_>| xs.into_iter().map(F::map(f)).collect())
    }
}

impl<F: Container<Ty>> Container<Ty> for Option<F> {
    type Input = F::Input;
    type With<X: Object<F::Input>> = Option<<F::With<X> as Object<Ty>>::Ty>;

    fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
        Box::new(|xs: Option<_>| xs.map(F::map(f)))
    }
}

impl<F: Container<Ty>, G: Container<Ty>> Container<Ty> for (F, G) {
    type Input = (F::Input, G::Input);
    type With<X: Object<Self::Input>> = (<F::With<X::Next0> as Object<Ty>>::Ty, <G::With<X::Next1> as Object<Ty>>::Ty);

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







macro_rules! generic_struct {
    (
        mod $mod:ident {
            $(
                $field:ident: ($obj_getter: ident),
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
                // type To<Other: Types>: Types<
                //     $(
                //         $field: Fn<(Self::$field,), Output = Other::$field>,
                //     )*
                // > = (
                //     $(
                //         Box<dyn Fn(Self::$field) -> Other::$field>,
                //     )*
                // );
            }

            // pub struct ConstTypes<A>(A);
            // impl<A: 'static> Types for ConstTypes<A> {
            //     $(
            //         type $field = A;
            //     )*
            // }
            type Tuple<T: Types> = (
                    $(
                        T::$field,
                    )*
            );

            // The (fibered) category created by the struct
            pub struct Pointwise<T: Types>(T);
            pub trait Cats = Types<
                $(
                    $field: super::Cat,
                )*
            >;
            impl<cats: Cats> Cat for Pointwise<cats> {
                type Get0 = Tuple<cats>;
                type DefaultObj = <Tuple<cats> as Cat>::DefaultObj;

                type Hom<X: Object<Self>, Y: Object<Self>> = All<(
                    $(
                        HomOf<cats::$field, <X::Next0 as Object<Tuple<cats>>>::$obj_getter, <Y::Next0 as Object<Tuple<cats>>>::$obj_getter>,
                    )*
                )>;

                // type GetN = TypesAsTyVec<cats>;
                // // <TypesAsTyVec<cats> as CatAtIndex<N>>::Result;
                // type Prev = ();
                // type Split1 = ();
                // type Split2 = ();
                // type Hom<X: Object<Self>, Y: Object<Self>> = All<(
                //     $(
                //         HomOf<AccessCat<TypesAsTyVec<cats>, $index>, Access<TypesAsTyVec<cats>, X::GetN, $index>, Access<TypesAsTyVec<cats>, Y::GetN, $index>>,
                //     )*
                // )>;
            }

            pub trait Objects<cats: Cats> = Types<
                $(
                    $field: Object<cats::$field>,
                )*
            >;
            impl<cats: Cats, Xs: Objects<cats>> Object<Pointwise<cats>> for Xs {
                type Ty = ();
                type Next0 = Tuple<Xs>;
            }



            // The functor created by a set of functors
            pub trait Containers<cats: Cats> = Types<
                $(
                    $field: super::Container<cats::$field>,
                )*
            >;

            pub type ContainersInputs<cats: Cats, C: Containers<cats>> = (
                $(
                    <C::$field as Container<cats::$field>>::Input,
                )*
            );

            impl<cats: Cats, C: Containers<cats>> Container<Pointwise<cats>> for C {
                type Input = Pointwise<ContainersInputs<cats, C>>;
                type With<X: Object<Self::Input>> = (
                    $(
                        <C::$field as Container<cats::$field>>::With<<X::Next0 as Object<Tuple<ContainersInputs<cats, C>>>>::$obj_getter>,
                    )*
                );

                fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> <Pointwise<cats> as Cat>::Hom<Self::With<X>, Self::With<Y>> {
                    All {
                        $(
                            $field: <C::$field as Container<cats::$field>>::map(&f.$field),
                        )*
                    }
                }
            }

            // --------- All container ------------
            pub struct Const<T>(T); 
            impl<T: 'static> Types for Const<T> {
                $(
                    type $field = T;
                )*
            }
            impl<Cs: Types<$($field: Container<Ty>,)*>> Container<Ty> for All<Cs> {
                type Input = Pointwise<ContainersInputs<Const<Ty>, Cs>>;
                type With<X: Object<Self::Input>> = All<(
                    $(
                        <
                            <Cs::$field as Container<Ty>>::With<<X::Next0 as Object<Tuple<ContainersInputs<Const<Ty>, Cs>>>>::$obj_getter>
                            as Object<Ty>
                        >::Ty,
                    )*
                )>;
                // All<<Cs as Container<Pointwise<Const<Ty>>>>::With<X>>;

                fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: &'static HomOf<Self::Input,X,Y>) -> HomOf<Ty,Self::With<X>, Self::With<Y>> {
                    Box::new(|input| {
                        All {
                            $(
                                $field: (<Cs::$field as Container<Ty>>::map(&f.$field))(input.$field),
                            )*
                        }
                    })
                }
                
            }


            /*
            pub struct AllContainers<cats: Cats, C: Containers<cats>>(pub std::marker::PhantomData<(cats, C)>);

            impl<cats: Cats, C: Containers<cats>> Container<Pointwise<cats>> for AllContainers<cats, C> {
                type Input = Pointwise<ContainersInputs<cats, C>>;

                type With<X: Object<Self::Input>> = All<(
                    $(
                        <C::$field as Container<AccessCat<TypesAsTyVec<cats>, $index>>>::With<X::GetN<$index>>,
                    )*
                )>;

                fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <Pointwise<cats> as Cat>::Hom<Self::With<X>, Self::With<Y>> {
                    All {
                        $(
                            $field: <C::$field as Container<cats::$field>>::map(f.$field),
                        )*
                    }
                }
            }
            */


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
            // impl<T: Types> All<T> {
            //     pub fn map<S: Types>(self, f: All<T::To<S>>) -> All<S> {
            //         All {
            //             $(
            //                 $field: (f.$field)(self.$field),
            //             )*
            //         }
            //     }
            // }

        }
    }
}








// impl<Cs: broker_info::Cats, Fs: broker_info::Containers<Cs>> Container<broker_info::Pointwise<Cs>> for Fs {
//     type Input = ();

//     type With<X: Object<Self::Input>> = ();

//     fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <broker_info::Pointwise<Cs> as Cat>::Hom<Self::With<X>, Self::With<Y>> {
//         todo!()
//     }
// }

// #[allow(nonstandard_style)]
// impl<cats: broker_info::Cats> Container<Ty> for broker_info::All<cats> {
//     type Input = broker_info::Pointwise<cats>;

//     type With<X: Object<Self::Input>> = broker_info::All<broker_info::ObjectTypes<cats, X>>;

//     fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
//         todo!()
//     }
// }

use crate::Asset;
use sp_runtime::AccountId32;
use cf_primitives::AssetAmount;
// use cf_utilities::rpc::NumberOrHex;
use sp_std::vec::Vec;

// --------------

generic_struct!{
    mod layer {
        AssetAmount: (Next0), BtcAddress: (Next1), AccountId: (Next2),
    }
}

pub struct RuntimeLayer;
pub struct RpcLayer;

impl layer::Types for RuntimeLayer {
    type AssetAmount = AssetAmount;
    type BtcAddress = u8;
    type AccountId = u32;
}

impl layer::Types for RpcLayer {
    type AssetAmount = u64;
    type BtcAddress = u16;
    type AccountId = u16;
}

// pub fn get_migration() -> layer::Hom<RuntimeLayer, RpcLayer> {
//     layer::All {
//         AssetAmount: Box::new(|a| NumberOrHex::Number(a as u64)),
//         BtcAddress: Box::new(|a| a as u16),
//         AccountId: todo!(),
//     }
// }


// ------------

generic_struct!{
    mod broker_info {
        earned_fees: (Next0),
        btc_vault_deposit_address: (Next1),
        affiliates: (Next2),
        bond: (Next3),
        bound_fee_withdrawal_address: (Next4),
    }
}

struct BrokerInfoShape;
impl broker_info::Types for BrokerInfoShape {
    type earned_fees = Vec<(Asset, !)>;
    type btc_vault_deposit_address = Option<!>;
    type affiliates = Vec<(AccountId32, !)>;
    type bond = (!, !);
    type bound_fee_withdrawal_address = Option<!>;
}

impl<L: layer::Types> broker_info::Types for L {
    type earned_fees = ((), L::AssetAmount, );
    type btc_vault_deposit_address = L::BtcAddress;
    type affiliates = ((), L::AccountId);
    type bond = (u8, u8,);
    type bound_fee_withdrawal_address = u16;
}

// now from a Hom<layer::Cat,L,K> I can get a brokerinfo::Types

type ShapeCat = <broker_info::All<BrokerInfoShape> as Container<Ty>>::Input;

fn domap() -> HomOf<ShapeCat, RuntimeLayer, RpcLayer> {
    let map_amount: Box<dyn Fn(AssetAmount) -> u64>
        = Box::new(|a: AssetAmount| a as u64);
    broker_info::All {
        earned_fees: ((), map_amount),
        btc_vault_deposit_address: Box::new(|abc: u8| abc.into()),
        affiliates: todo!(),
        bond: todo!(),
        bound_fee_withdrawal_address: todo!(),
    }
}

/*
type ShapeCat = <broker_info::AllContainers<broker_info::ConstTypes<Ty>, BrokerInfoShape> as Container<broker_info::Pointwise<broker_info::ConstTypes<Ty>>>>::Input;


fn accept_functor<T: Container<broker_info::Pointwise<broker_info::ConstTypes<Ty>>>, X: Object<T::Input>, Y: Object<T::Input>>(x: T, f: HomOf<T::Input,X,Y>) {

}

fn myresult() {
    accept_functor(broker_info::AllContainers::<_, BrokerInfoShape>(Default::default()), domap());

    // let x: broker_info::Hom<RuntimeLayer, RpcLayer> = 
}
 */

// fn mymap<K: layer::Types, L: layer::Types>(f: layer::Hom<K, L>) -> broker_info::Hom<K,L>{
//     broker_info::All {
//         earned_fees: (todo!(), todo!()),
//         btc_vault_deposit_address: todo!(),
//         affiliates: todo!(),
//         bond: todo!(),
//         bound_fee_withdrawal_address: todo!(),
//     }
// }

// type BrokerInfo<L: layer::Types> = broker_info::FilledShape<BrokerInfoShape, L>;

// pub fn mymap(x: BrokerInfo<RuntimeLayer>) -> BrokerInfo<RpcLayer> {

/*

// ------------------ numbers and vecs ---------------------
trait Num: Sized {
    type Prev;
    type RecCat<Z: CatAt<Zero>, S: CatSucMap>: CatAt<Self>;
    type RecObj<cats: IndexedCat, Z: ObjAt<Zero, cats>, S: ObjSucMap<cats>>: ObjAt<Self, cats>; 
}
trait CatAt<N: Num> {
    type Result: Cat;
}
trait ObjAt<N: Num, cats: IndexedCat> {
    type Result: Object<cats::Get<N>>;
}
// trait TypeInfo {
//     type Category: PointedCat;
// }
// impl TypeInfo for () {
//     type Category = ();
// }
// trait Typed<T: TypeInfo> {
//     type AsTy;
//     type AsCat: Cat;
//     type AsObj: Object<T::Category>;
// }
// struct IsObject<C: Cat>(C);
// impl<C: Cat> TypeInfo for IsObject<C> {
//     type Category = C;
// }

trait CatSucMap {
    type Get<N: Num, X: CatAt<N>>: CatAt<Suc<N>>;
}
trait ObjSucMap {
    type Get<N: Num, C: Cat, cats: IndexedCat, X: ObjAt<N, cats>>: ObjAt<Suc<N>, HCons<C,cats>>;
}

    // type RecObj<cats: IndexedCat, Z: ObjAt<Zero, cats::Get<Zero>>>; 
    

struct Suc<N>(sp_std::marker::PhantomData<N>);
impl<N: Num> Num for Suc<N> { 
    type Prev = N;
    type RecCat<Z: CatAt<Zero>, S: CatSucMap> = S::Get<N, N::RecCat<Z, S>>;
    // type RecObj<C: Cat, Z: ObjAt<Zero, C>, S: ObjSucMap<C>> = S::Get<N, N::RecObj<C, Z, S>>;
    type RecObj<cats: IndexedCat, Z: ObjAt<Zero, cats>, S: ObjSucMap> = S::Get<N, N::RecObj<cats, Z, S>>;
    // type RecObj<cats: IndexedCat, Z: ObjAt<Zero, cats::Get<Zero>>>; 
}
struct Zero;
impl Num for Zero {
    type Prev = Zero;
    type RecCat<Z: CatAt<Zero>, S: CatSucMap> = Z;
    type RecObj<cats: IndexedCat, Z: ObjAt<Zero, cats>, S: ObjSucMap<cats>> = Z;
    // type RecObj<C: Cat, Z: ObjAt<Zero, C>, S: ObjSucMap<C>> = Z;
    // type RecObj<cats: IndexedCat, Z: ObjAt<Zero, cats::Get<Zero>>, S: ObjSucMap<cats>> = Z; 
}

// -- HVec

trait HVec {
    type N: Num;
    type Head;
    type Tail: HVec;
}
struct HCons<A, As>(A, As);
struct HNil;
impl HVec for HNil {
    type N = Zero;
    type Head = ();
    type Tail = HNil;
}
impl<A, As: HVec> HVec for HCons<A, As> {
    type N = Suc<As::N>;
    type Head = A;
    type Tail = As;
}

// base impls for cat
struct Impl<Ty>(Ty);
impl<A: Cat, As> CatAt<Zero> for Impl<HCons<A,As>> {
    type Result = A;
}
impl<A, As: CatAt<N>, N: Num> CatAt<Suc<N>> for Impl<HCons<A,As>> {
    type Result = As::Result;
}

// base impls for obj
impl<cats: IndexedCat, A: Object<cats::Get<Zero>>, As> ObjAt<Zero, cats> for Impl<HCons<A,As>> {
    type Result = A;
}
impl<C: Cat, cats: IndexedCat, A, As: ObjAt<N, cats>, N: Num> ObjAt<Suc<N>, HCons<C,cats>> for Impl<HCons<A,As>> {
    type Result = As::Result;
}

// generic impl for cat
struct HCons1<A>(A);
impl<A> CatSucMap for HCons1<A> {
    type Get<N: Num, X: CatAt<N>> = Impl<HCons<A, X>>;
}
impl<A: Cat, As, N: Num> CatAt<N> for HCons<A,As> {
    type Result = <N::RecCat<Impl<Self>, HCons1<A>> as CatAt<N>>::Result;
}

// generic impl for obj
impl<C: Cat, A> ObjSucMap<C> for HCons1<A> {
    type Get<N: Num, X: ObjAt<N, C>> = Impl<HCons<A,X>>;
    // type Get<N: Num, X: ObjAt<N, cats::>> = Impl<HCons<A, X>>;
}
impl<C: Cat, A, As, N: Num> ObjAt<N, C> for HCons<A,As> {
    type Result = <N::RecObj<C, Impl<Self>, HCons1<A>> as ObjAt<N, C>>::Result;
}


trait Indexed {
    type Get<N: Num>;
}
trait IndexedCat {
    type Get<N: Num>: Cat;
}
// impl<A,As> Indexed for HCons<A,As> {
//     type Get<N: Num> = <Self as CatAt<N, ()>>::Result;
// }
impl<A: Cat, As> IndexedCat for HCons<A,As> {
    type Get<N: Num> = <Self as CatAt<N>>::Result;
}
// impl<A> Indexed for A {
//     type Get<N: Num> = <<Self as At<N, ()>>::Result as Typed<()>>::AsTy;
// }
// impl<A> IndexedCat for A {
//     type Get<N: Num> = <<Self as At<N, ()>>::Result as Typed<()>>::AsCat;
// }
trait IndexedObj<C: IndexedCat> {
    type Get<N: Num>: Object<C::Get<N>>;
}
impl<cats: IndexedCat, A: Object<cats::Get<Zero>>, As> IndexedObj<cats> for HCons<A,As> {
    type Get<N: Num> = <Self as ObjAt<N, cats::Get<N>>>::Result;
}

// impl<A: Typed<IsObject<cats::Get<Zero>>>,As, cats: IndexedCat> IndexedObj<cats> for HCons<A,As> {
//     type Get<N: Num> = <<Self as CatAt<N, IsObject<cats::Get<N>>>>::Result as Typed<()>>::AsObj;
// }

// impl<A, cats: IndexedCat> IndexedObj<cats> for A {
//     type Get<N: Num> = <<Self as At<N, IsObject<cats::Get<N>>>>::Result as Typed<()>>::AsObj;
// }



// impl<A: Typed<()>,As> IndexedCat for HCons<A,As> {
//     type Get<N: Num> = <<Self as At<N, ()>>::Result as Typed<()>>::AsCat;
// }



// impl<A, As: HVec, N: Num> At<N> for HCons<A,As> {
//     type Result = N::Rec<Self, ;
// }


// ---------------- again new ------------------------


// ---------------- new indexes ----------------------

// trait SameSize<A>: Sized {
//     type MapIndex<I: IndexFor<A>>: IndexFor<Self>;
// }

// impl<A, B, As: HVec, Bs: HVec + SameSize<As>> SameSize<HCons<A,As>> for HCons<B,Bs> {
//     type MapIndex<I: IndexFor<HCons<A,As>>>;
// }

// trait Indexed {
//     type Shape;
// }

// trait IndexFor<Shape> {
//     type Get: 'static;
// }

// impl<A: 'static, As: HVec> IndexFor<HCons<A,As>> for Zero {
//     type Get = A;
// }
// struct Translate<N>(N);

// impl<A: 'static, As: HVec> IndexFor<HCons<A,As>> for Zero {
//     type Get = A;
// }



// impl<As: HVec> IndexFor<HCons<!, As>> for Zero {
//     type Get<T: Indexed<Shape = Self>>;
// }


// trait IndexedTy {
//     type At<N: IndexFor<Self>>: 'static;
// }
// impl<X> IndexedTy for X {
//     type At<N: IndexFor<Self>> = N::Get;
// }
// trait IndexForCat<Cs> = IndexFor<Cs, Get: Cat>;
// trait IndexedCat {
//     type At<N: IndexForCat<Self>>: Cat;
// }
// impl<Cs> IndexedCat for Cs {
//     type At<N: IndexForCat<Self>> = N::Get;
// }

// trait IndexForObj<Objs, C: Cat> = IndexFor<Objs, Get: Object<C>>;


/*


// ---------------- old indexes ----------------------

trait PointedCat: Cat + Sized {
    type Pt: Object<Self>;
}

// -- indexing vecs

trait AtIndex<N: ?Sized, C: Cat> {
    type Result: 'static + Object<C>;
}
impl<X: 'static + Object<C>, Xs: HVec, C: Cat> AtIndex<Zero, C> for HCons<X, Xs> {
    type Result = X;
}
impl<X, Xs: HVec + AtIndex<N, C>, N, C: Cat> AtIndex<Suc<N>, C> for HCons<X, Xs> 
{
    type Result = Xs::Result;
}
// impl<X, N: ?Sized, C: PointedCat> AtIndex<N, C> for X {
//     default type Result = C::Pt;
// }


// trait ObjAtIndex<N: ?Sized, C: Cat> = AtIndex<N, Result: Object<C>>;
trait AllAtIndex<Cs: AllCatAtIndex> {
    type Get<N: ?Sized>: 'static + Object<Cs::Get<N>>
    where 
        Cs: CatAtIndex<N>,
        Self: AtIndex<N, Cs::Get<N>>;
}
impl<X, Cs: AllCatAtIndex> AllAtIndex<Cs> for X 
{
    type Get<N: ?Sized> = <X as AtIndex<N, Cs::Get<N>>>::Result
        where 
            Cs: CatAtIndex<N>,
            X: AtIndex<N, Cs::Get<N>>;
}
// trait AllObjAtIndex<T: AllCatAtIndex> {
//     type Get<N: ?Sized>: ObjAtIndex<N, <T::Get<N> as CatAtIndex<N>>::Result>;
// }
// impl<X, T: AllCatAtIndex> AllObjAtIndex<T> for X
// {
//     type Get<N: ?Sized> = <X as AtIndex<N>>::Result;
//         // <X as ObjAtIndex<N <T::Get<N>>::Result> 
// }
// type Access<Cs: AllCatAtIndex, Xs: AllAtIndex<Cs>, N: ?Sized> = <Xs::Get<N> as AtIndex<N, AccessCat<Cs, N>>>::Result;
type Access<Cs: AllCatAtIndex, Xs: AllAtIndex<Cs>, N: ?Sized> = Xs::Get<N>;
//  as AtIndex<N, AccessCat<Cs, N>>>::Result;


trait CatAtIndex<N: ?Sized> {
    type Result: Cat;
}
impl<X: Cat, Xs: HVec> CatAtIndex<Zero> for HCons<X, Xs> {
    type Result = X;
}
impl<X, Xs: HVec + CatAtIndex<N>, N> CatAtIndex<Suc<N>> for HCons<X, Xs> 
{
    type Result = Xs::Result;
}
// impl<X, N: ?Sized> CatAtIndex<N> for X {
//     default type Result = ();
// }
trait AllCatAtIndex {
    type Get<N: ?Sized>: Cat where Self: CatAtIndex<N>;
}

impl<X> AllCatAtIndex for X {
    type Get<N: ?Sized> = <X as CatAtIndex<N>>::Result where Self: CatAtIndex<N>;
}
type AccessCat<Cs: AllCatAtIndex, N: ?Sized> = Cs::Get<N>;

// I have to prove that if I have (cats: CatAtIndex<N>) then I also have GetN: AtIndex<N, Xs::Get<N>>


// impl<X> AllCatAtIndex for X
// where for<N: Sized> X: CatAtIndex<N>
// {
    
// }

// pub fn myfun() {
//     type Test = HCons<u8, HCons<u16, HNil>>;
//     type R2 = <Test as AtIndex<Suc<Zero>>>::Result;

//     let x: R2 = 1u8;
// }

macro_rules! hvec_for_tuple {
    (
        $head:ty,
        $(
            $tail:ty,
        )*
    ) => {
        HCons<$head, hvec_for_tuple!($($tail, )*)>
    };
    () => {
        HNil
    }
}


// ----------------------- categories ------------------------


impl PointedCat for () {
    type Pt = ();
}
impl<N: Cat, M: Cat> Cat for (N, M) {
    type Prev = ();
    type Split1 = N;
    type Split2 = M;
    type Hom<X: Object<Self>, Y: Object<Self>> = (N::Hom<X::Split1, Y::Split1>, M::Hom<X::Split2, Y::Split2>);
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
    type GetN = ();
}



pub fn mytest() {
    type MyCats = hvec_for_tuple!(Ty, Ty, Ty,);
    type MyTy = hvec_for_tuple!(u16, u8, bool, );
    fn myfunction<X: Object<AccessCat<MyCats, Suc<Zero>>>>(x: X) {

    }
    myfunction::<u8>(0u8);
    // let x: Access<MyCats, MyTy, Zero> = 0u16;
}

// X::GetN<Zero> : (Pointwise<ContainersInputs<cats, C>>::GetN<Zero>)
//               = AccessCat<TypesAsTyVec<ContainersInputs<cats, C>>, Zero>
//               = 

macro_rules! generic_struct {
    (
        mod $mod:ident {
            $(
                $field:ident: ($index:ty),
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
            type TypesAsTyVec<T: Types> = hvec_for_tuple!(
                $(
                    T::$field,
                )*
            );
            // pub type Hom<T: Cats, S: Cats> = All<T::To<S>>;

            pub struct ConstTypes<A>(A);
            impl<A: 'static> Types for ConstTypes<A> {
                $(
                    type $field = A;
                )*
            }

            // The (fibered) category created by the struct
            pub struct Pointwise<T: Types>(T);
            pub trait Cats = Types<
                $(
                    $field: super::Cat,
                )*
            > where
                TypesAsTyVec<Self>: AllCatAtIndex
            ;
            impl<cats: Cats> Cat for Pointwise<cats> {
                type GetN = TypesAsTyVec<cats>;
                // <TypesAsTyVec<cats> as CatAtIndex<N>>::Result;
                type Prev = ();
                type Split1 = ();
                type Split2 = ();
                type Hom<X: Object<Self>, Y: Object<Self>> = All<(
                    $(
                        HomOf<AccessCat<TypesAsTyVec<cats>, $index>, Access<TypesAsTyVec<cats>, X::GetN, $index>, Access<TypesAsTyVec<cats>, Y::GetN, $index>>,
                    )*
                )>;
            }
            pub struct ObjectTypes<cats: Cats, X: Object<Pointwise<cats>>>(std::marker::PhantomData<(cats, X)>);
            impl<cats: Cats, X: Object<Pointwise<cats>>> Types for ObjectTypes<cats, X> {
                $(
                    type $field = (); // TODO
                )*
            }
            pub trait Objects<cats: Cats> = Types<
                $(
                    $field: Object<AccessCat<TypesAsTyVec<cats>, $index>>,
                )*
            > where
                TypesAsTyVec<Self>: AllAtIndex<cats>,
                // <TypesAsTyVec<Self> as AllAtIndex>::Result: Object<<TypesAsTyVec<cats> as CatAtIndex<N>>::Result>,
            ;
            impl<cats: Cats, Xs: Objects<cats>> Object<Pointwise<cats>> for All<Xs> {
                type GetN = TypesAsTyVec<Xs>;
                // <<TypesAsTyVec<Xs> as AllAtIndex<TypesAsTyVec<cats>>>::Get<N> as AtIndex<N, 
                //     <<TypesAsTyVec<cats> as AllCatAtIndex>::Get<N> as CatAtIndex<N>>::Result
                // >>::Result;
                type Get = ();
                type Next = ();
                type Split1 = ();
                type Split2 = ();
            }

            // The functor created by a set of functors
            pub trait Containers<cats: Cats> = Types<
                $(
                    $field: super::Container<AccessCat<TypesAsTyVec<cats>, $index>>,
                )*
            >;
            pub type ContainersInputs<cats: Cats, C: Containers<cats>> = (
                $(
                    <C::$field as Container<AccessCat<TypesAsTyVec<cats>, $index>>>::Input,
                )*
            );

            pub struct AllContainers<cats: Cats, C: Containers<cats>>(pub std::marker::PhantomData<(cats, C)>);

            impl<cats: Cats, C: Containers<cats>> Container<Pointwise<cats>> for AllContainers<cats, C> {
                type Input = Pointwise<ContainersInputs<cats, C>>;

                type With<X: Object<Self::Input>> = All<(
                    $(
                        <C::$field as Container<AccessCat<TypesAsTyVec<cats>, $index>>>::With<X::GetN<$index>>,
                    )*
                )>;

                fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <Pointwise<cats> as Cat>::Hom<Self::With<X>, Self::With<Y>> {
                    All {
                        $(
                            $field: <C::$field as Container<cats::$field>>::map(f.$field),
                        )*
                    }
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
        AssetAmount: (Zero), BtcAddress: (Suc<Zero>), AccountId: (Suc<Suc<Zero>>),
    }
}

generic_struct!{
    mod broker_info {
        earned_fees: (Zero),
        btc_vault_deposit_address: (Suc<Zero>),
        affiliates: (Suc<Suc<Zero>>),
        bond: (Suc<Suc<Suc<Zero>>>),
        bound_fee_withdrawal_address: (Suc<Suc<Suc<Suc<Zero>>>>),
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



// impl<Cs: broker_info::Cats, Fs: broker_info::Containers<Cs>> Container<broker_info::Pointwise<Cs>> for Fs {
//     type Input = ();

//     type With<X: Object<Self::Input>> = ();

//     fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <broker_info::Pointwise<Cs> as Cat>::Hom<Self::With<X>, Self::With<Y>> {
//         todo!()
//     }
// }

// #[allow(nonstandard_style)]
// impl<cats: broker_info::Cats> Container<Ty> for broker_info::All<cats> {
//     type Input = broker_info::Pointwise<cats>;

//     type With<X: Object<Self::Input>> = broker_info::All<broker_info::ObjectTypes<cats, X>>;

//     fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(f: HomOf<Self::Input,X,Y>) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
//         todo!()
//     }
// }


struct BrokerInfoShape;
impl broker_info::Types for BrokerInfoShape {
    type earned_fees = Vec<(Asset, !)>;
    type btc_vault_deposit_address = Option<!>;
    type affiliates = Vec<(AccountId32, !)>;
    type bond = (!, !);
    type bound_fee_withdrawal_address = Option<!>;
}

impl<L: layer::Types> broker_info::Types for L {
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

type ShapeCat = <broker_info::AllContainers<broker_info::ConstTypes<Ty>, BrokerInfoShape> as Container<broker_info::Pointwise<broker_info::ConstTypes<Ty>>>>::Input;


fn accept_functor<T: Container<broker_info::Pointwise<broker_info::ConstTypes<Ty>>>, X: Object<T::Input>, Y: Object<T::Input>>(x: T, f: HomOf<T::Input,X,Y>) {

}

fn myresult() {
    accept_functor(broker_info::AllContainers::<_, BrokerInfoShape>(Default::default()), domap());

    // let x: broker_info::Hom<RuntimeLayer, RpcLayer> = 
}

// fn mymap<K: layer::Types, L: layer::Types>(f: layer::Hom<K, L>) -> broker_info::Hom<K,L>{
//     broker_info::All {
//         earned_fees: (todo!(), todo!()),
//         btc_vault_deposit_address: todo!(),
//         affiliates: todo!(),
//         bond: todo!(),
//         bound_fee_withdrawal_address: todo!(),
//     }
// }

// type BrokerInfo<L: layer::Types> = broker_info::FilledShape<BrokerInfoShape, L>;

// pub fn mymap(x: BrokerInfo<RuntimeLayer>) -> BrokerInfo<RpcLayer> {



// }


// generic_struct_instance!{
//     struct BrokerInfo2<L: layer>: broker_info {
//         earned_fees: [u8] for Vec<!>,
//         btc_vault_deposit_address: [Id] for L::BtcAddress,
//         affiliates: Vec<u16>,
//         bond: L::AssetAmount,
//         bound_fee_withdrawal_address: L::BtcAddress,
//     }
// }






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


 */