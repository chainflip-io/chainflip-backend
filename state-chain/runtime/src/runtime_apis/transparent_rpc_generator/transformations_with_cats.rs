
use sp_std::boxed::Box;

// ------------------ categories & objects --------------------------

pub trait Cat: 'static + Sized {
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
pub type HomOf<C: Cat, X: Object<C>, Y: Object<C>> = C::Hom<X, Y>;

impl Cat for () {
	type DefaultObj = ();
}

pub trait Object<C: Cat>: 'static {
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

pub struct Ty;

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


// ------------------ containers --------------------------

pub trait Container<Output: Cat> {
	type Input: Cat + 'static;
	type With<X: Object<Self::Input>>: Object<Output>;
	fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(
		f: &'static HomOf<Self::Input, X, Y>,
	) -> Output::Hom<Self::With<X>, Self::With<Y>>;
}

impl Container<Ty> for ! {
	type Input = Ty;

	type With<X: Object<Ty>> = X;

	fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(
		f: &'static HomOf<Self::Input, X, Y>,
	) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
		Box::new(|x| f(x))
	}
}

impl<F: Container<Ty>> Container<Ty> for Vec<F> {
	type Input = F::Input;
	type With<X: Object<F::Input>> = Vec<<F::With<X> as Object<Ty>>::Ty>;

	fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(
		f: &'static HomOf<Self::Input, X, Y>,
	) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
		Box::new(|xs: Vec<_>| xs.into_iter().map(F::map(f)).collect())
	}
}

impl<F: Container<Ty>> Container<Ty> for Option<F> {
	type Input = F::Input;
	type With<X: Object<F::Input>> = Option<<F::With<X> as Object<Ty>>::Ty>;

	fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(
		f: &'static HomOf<Self::Input, X, Y>,
	) -> <Ty as Cat>::Hom<Self::With<X>, Self::With<Y>> {
		Box::new(|xs: Option<_>| xs.map(F::map(f)))
	}
}

impl<F: Container<Ty>, G: Container<Ty>> Container<Ty> for (F, G) {
	type Input = (F::Input, G::Input);
	type With<X: Object<Self::Input>> =
		(<F::With<X::Next0> as Object<Ty>>::Ty, <G::With<X::Next1> as Object<Ty>>::Ty);

	fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(
		f: &'static HomOf<Self::Input, X, Y>,
	) -> HomOf<Ty, Self::With<X>, Self::With<Y>> {
		Box::new(|(x, y)| (F::map(&f.0)(x), G::map(&f.1)(y)))
	}
}

#[duplicate::duplicate_item(T; [u32]; [u8]; [u16])]
impl Container<Ty> for T {
	type Input = ();

	type With<X: Object<Self::Input>> = T;

	fn map<X: Object<Self::Input>, Y: Object<Self::Input>>(
		_f: &'static HomOf<Self::Input, X, Y>,
	) -> HomOf<Ty, Self::With<X>, Self::With<Y>> {
		Box::new(|x| x)
	}
}

#[macro_export]
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

            use crate::runtime_apis::transparent_rpc_generator::transformations::*;
            use crate::runtime_apis::transparent_rpc_generator::type_variants::*;


            pub trait Types: 'static {
                $(
                    type $field;
                )*
            }

            type Tuple<T: Types> = (
                    $(
                        T::$field,
                    )*
            );

            // The (fibered) category created by the struct
            pub struct Pointwise<T: Types>(T);
            pub trait Cats = Types<
                $(
                    $field: Cat,
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
                    $field: Container<cats::$field>,
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

            // --------- Migrations -------------
            impl<
                T: Types, S: Types, M:
                $(
                  TypedMigration<T::$field, S::$field> +
                )*
            > TypedMigration<All<T>,All<S>> for M {

                fn forwards(x: All<T>) -> All<S> {
                    All {
                        $(
                            $field: M::forwards(x.$field),
                        )*
                    }
                }

                fn backwards(x: All<S>) -> All<T> {
                    All {
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
pub use generic_struct;