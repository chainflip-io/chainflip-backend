// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0
pub mod __external {
	pub use codec;
	pub use serde::{Deserialize, Serialize};
}

/// Adds #[derive] statements for commonly used traits. These are currently: Debug, Clone,
/// PartialEq, Eq, Encode, Decode, Serialize, Deserialize
#[macro_export]
macro_rules! derive_common_traits {
	($($Definition:tt)*) => {
		#[derive(
			Debug, Clone, PartialEq, Eq, $crate::__external::codec::Encode, $crate::__external::codec::Decode, $crate::__external::codec::DecodeWithMemTracking,
		)]
		#[derive($crate::__external::Deserialize, $crate::__external::Serialize)]
		#[serde(bound(deserialize = "", serialize = ""))]
		$($Definition)*
	};
}
pub use derive_common_traits;

/// Adds #[derive] statements for commonly used traits, *without* adding bounds on eventual type
/// parameters. The implemented traits are: Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize,
/// Deserialize.
#[macro_export]
macro_rules! derive_common_traits_no_bounds {
	($($Definition:tt)*) => {
		#[derive_where::derive_where(
			Debug, Clone, PartialEq, Eq;
		)]
		#[derive($crate::__external::codec::Encode, $crate::__external::codec::Decode, $crate::__external::codec::DecodeWithMemTracking)]
		#[codec(encode_bound())]
		#[derive($crate::__external::Deserialize, $crate::__external::Serialize)]
		#[serde(bound(deserialize = "", serialize = ""))]
		$($Definition)*
	};
}
pub use derive_common_traits_no_bounds;

/// Adds #[derive] statements for commonly used traits, including `Validate`. Automatically
/// generates the body of the struct, containing just a PhantomData with all type variables.
/// Use like:
///
/// use utilities::macros::define_empty_struct;
/// define_empty_struct! {
///     struct SomeStruct<A: Ord, B: 'static>;
/// }
#[macro_export]
macro_rules! define_empty_struct {
	(
		[$name:ident $(: $path:path)?, $($rest:tt)*]
		[$($names:tt)*]
		[$($names_and_bounds:tt)*]
		$(#[$meta:meta])*
		$vis:vis struct $struct_name:ident
	) => {
		cf_utilities::define_empty_struct!{
			[$($rest)*]
			[$($names)* $name, ]
			[$($names_and_bounds)* $name $(:$path)?, ]
			$(#[$meta])*
			$vis struct $struct_name
		}
	};
	(
		[$name:ident: $l:lifetime, $($rest:tt)*]
		[$($names:tt)*]
		[$($names_and_bounds:tt)*]
		$(#[$meta:meta])*
		$vis:vis struct $struct_name:ident
	) => {
		cf_utilities::define_empty_struct!{
			[$($rest)*]
			[$($names)* $name, ]
			[$($names_and_bounds)* $name:$l, ]
			$(#[$meta])*
			$vis struct $struct_name
		}
	};

	// handling the last entry
	( [$name:ident $(: $path:path)? >;]  $($rest:tt)* ) => { cf_utilities::define_empty_struct!{ [ $name $(:$path)?, >; ] $($rest)* }};
	( [$name:ident: $l:lifetime >;] $($rest:tt)* ) => { cf_utilities::define_empty_struct!{ [ $name:$l, >; ] $($rest)* }};

	// the main branch
	(
		[>;]
		[$($names:tt)*]
		[$($names_and_bounds:tt)*]
		$(#[$meta:meta])*
		$vis:vis struct $struct_name:ident
	) => {
		cf_utilities::derive_common_traits!{
			#[derive(scale_info::TypeInfo, frame_support::DefaultNoBound)]
			#[scale_info(skip_type_params(T, I))]
			$(#[$meta])*
			$vis struct $struct_name<$($names_and_bounds)*>
			(
				sp_std::marker::PhantomData
				<
				($($names)*)
				>
			);

			impl<$($names_and_bounds)*> cf_traits::Validate for $struct_name<$($names)*> {
				type Error = ();

				fn is_valid(&self) -> Result<(), Self::Error> {
					Ok(())
				}
			}
		}
	};
	// This is a special case handling structs without type parameters
	(
		$(#[$meta:meta])* $vis:vis struct $struct_name:ident;
	) => {
		cf_utilities::derive_common_traits!{
			#[derive(scale_info::TypeInfo, frame_support::DefaultNoBound)]
			#[derive(PartialOrd, Ord)]
			$(#[$meta])*
			$vis struct $struct_name {}
		}
		#[cfg(test)]
		impl proptest::prelude::Arbitrary for $struct_name {
			type Parameters = ();
			type Strategy = impl proptest::prelude::Strategy<Value = Self> + Clone + Sync + Send;
			fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
				proptest::prelude::Just(Default::default())
			}
		}
		impl cf_traits::Validate for $struct_name {
			type Error = ();
			fn is_valid(&self) -> Result<(), Self::Error> {
				Ok(())
			}
		}
	};
	// This is the entry point for structs with type parameters
	(
		$(#[$meta:meta])* $vis:vis struct $struct_name:ident<$($rest:tt)*
	) => {
		cf_utilities::define_empty_struct!{
			[$($rest)*]
			[]
			[]
			$(#[$meta])*
			$vis struct $struct_name
		}
	};
}
pub use define_empty_struct;

/// Syntax sugar for implementing multiple traits for a single type.
/// All attributes (e.g. `#[doc]`, `#[async_trait::async_trait]`, etc.) placed
/// before a trait block are forwarded to the generated `impl` item.
///
/// Example use:
///
/// impls! {
///     for u8:
///     Clone {
///         ...
///     }
///     Copy {
///         ...
///     }
///     #[async_trait::async_trait]
///     SomeAsyncTrait {
///         ...
///     }
/// }
pub macro impls {
	// trait implementation
    (for $name:ty $(where ($($bounds:tt)*))? :
	$(#[$meta:meta])* $($trait:ty)?  $(where ($($trait_bounds:tt)*))? {$($trait_impl:tt)*}
	$($rest:tt)*
	) => {
        $(#[$meta])*
        impl$(<$($bounds)*>)? $($trait for)? $name
		$(where $($trait_bounds)*)?
		{
            $($trait_impl)*
        }
        impls!{for $name $(where ($($bounds)*))? : $($rest)*}
    },
	// end of implementations
    (for $name:ty $(where ($($bounds:tt)*))? :) => {}
}

/// Syntax sugar for implementing multiple hooks for a single type.
/// All attributes placed before a hook block are forwarded to the generated
/// `impl` item.
///
/// Example use:
///
/// ```ignore
/// hook_impls! {
///     for MyStruct<A> where (A: Clone):
///
///     fn(&mut self, (fst, snd): (A,A)) -> A {
///         fst
///     }
///
///     fn(&mut self, _input: ()) -> A {
///         ...
///     }
/// }
/// ```
pub macro hook_impls {
	// hook implementation
    (for $name:ty $(where ($($bounds:tt)*))? :
	$(#[$meta:meta])* fn(&mut $self:ident, $args:tt: $input_ty:ty) -> $output_ty:ty
	$(where ($($trait_bounds:tt)*))? {$($trait_impl:tt)*}
	$($rest:tt)*
	) => {
        $(#[$meta])*
        impl$(<$($bounds)*>)? cf_traits::Hook<($input_ty, $output_ty)> for $name
		$(where $($trait_bounds)*)?
		{
			fn run(&mut $self, $args: $input_ty) -> $output_ty {
            	$($trait_impl)*
			}
        }
        hook_impls!{for $name $(where ($($bounds)*))? : $($rest)*}
    },
	// end of implementations
    (for $name:ty $(where ($($bounds:tt)*))? :) => {}
}

#[macro_export]
/// This macro prevents `cargo fmt` from messing up the formatting.
macro_rules! cargo_fmt_ignore {
	($($tokens:tt)*) => {
		$($tokens)*
	};
}

#[macro_export]
macro_rules! or_else {
    (() or ($($else:tt)*)) => {
        $($else)*
    };
    (($($tt:tt)*) or ($($else:tt)*)) => {
        $($tt)*
    };
}
pub use or_else;

pub macro eval_macro_expr {
    (if () {$($true:tt)*} else {$($false:tt)*}) => {
        $crate::macros::eval_macro_expr!{$($false)*}
    },

    (if ($($cond:tt)*) {$($true:tt)*} else {$($false:tt)*}) => {
        $crate::macros::eval_macro_expr!{$($true)*}
    },

    ($($rest:tt)*) => {
        $($rest)*
    },
}

pub macro better_modules {

    // ----------------------
    // case 0: `type X = Y`
    // ----------------------
    (
        next = {
            $vis:vis type $name:ident = $ty:ty;
            $($rest:tt)*
        }
        tele = {$(($tele:ident: $($tele_type:tt)*))*}
    ) => {
        // add tele
        $vis type $name<$($tele: $($tele_type)*),*> = $ty;

        // recursive call:
        $crate::macros::better_modules! {
            next = {$($rest)*}
            tele = {$(($tele: $($tele_type)*))*}
        }
    },

    // ----------------------
    // case 1: `struct X { ... }`
    // ----------------------
    (
        next = {
            $vis:vis struct $name:ident $(< $($T:ident $(: $TBound:path)?),+ >)? { $($content:tt)* }
            $($rest:tt)*
        }
        tele = {$(($tele:ident: $($tele_type:tt)*))*}
    ) => {

        use sp_std::marker::PhantomData;

        // define struct with additional tele bounds
        $vis struct $name< $( $($T $(: $TBound)?,)+ )? $($tele: $($tele_type)*),* > {
            $($content)*

            // add phantom for tele types
            _phantom: PhantomData<($($tele,)*)>,
        }

        // recursive call:
        $crate::macros::better_modules! {
            next = {$($rest)*}
            tele = {$(($tele: $($tele_type)*))*}
        }
    },

    // ----------------------
    // case 2: `impl Trait for Type { ... }`
    // ----------------------
    (
        next = {
            impl $(< $($T:ident $(: $TBound:path)?),+ >)? $($trait_path:ident)::+ $(< $($trait_generic:ty),+ $(,)? >)? for $name:ty { $($content:tt)* }
            $($rest:tt)*
        }
        tele = {$(($tele:ident: $($tele_type:tt)*))*}
    ) => {
        impl< $( $($T $(: $TBound)?,)+ )? $($tele: $($tele_type)*),* > $($trait_path)::+ $(< $($trait_generic),+ >)? for $name {
            duplicate::substitute! {
                [
                    Parameters [ $($tele, )* ]
                ]
                $($content)*
            }
        }

        // recursive call:
        $crate::macros::better_modules! {
            next = {$($rest)*}
            tele = {$(($tele: $($tele_type)*))*}
        }
    },

    // ----------------------
    // case 3: `mod name {}`
    // ----------------------
    (
        next = {
            $vis:vis mod $name:ident { $($content:tt)* }
            $($rest:tt)*
        }
        tele = {$(($tele:ident: $($tele_type:tt)*))*}
    ) => {
        // continue inside module
        $vis mod $name {
            $crate::macros::better_modules! {
                next = { $($content)* }
                tele = { $(($tele: $($tele_type)*))* }
            }
        }

        // recursive call:
        $crate::macros::better_modules! {
            next = {$($rest)*}
            tele = {$(($tele: $($tele_type)*))*}
        }
    },

    // ----------------------
    // case 4: `if () { ... } else { ... }`
    // ----------------------
    (
        next = {
            if () {$($true:tt)*} else {$($false:tt)*}
            $($rest:tt)*
        }
        tele = {$(($tele:ident: $($tele_type:tt)*))*}
    ) => {
        // pick false branch and continue inside
        $crate::macros::better_modules! {
            next = { $($false)* }
            tele = { $(($tele: $($tele_type)*))* }
        }

        // recursive call:
        $crate::macros::better_modules! {
            next = {$($rest)*}
            tele = {$(($tele: $($tele_type)*))*}
        }
    },

    // ----------------------
    // case 5: `if (something) { ... } else { ... }`
    // ----------------------
    (
        next = {
            if ($($cond:tt)+) {$($true:tt)*} else {$($false:tt)*}
            $($rest:tt)*
        }
        tele = {$(($tele:ident: $($tele_type:tt)*))*}
    ) => {
        // pick true branch and continue inside
        $crate::macros::better_modules! {
            next = { $($true)* }
            tele = { $(($tele: $($tele_type)*))* }
        }

        // recursive call:
        $crate::macros::better_modules! {
            next = {$($rest)*}
            tele = {$(($tele: $($tele_type)*))*}
        }
    },

    // ----------------------
    // case 6: any other item is emitted as is
    // ----------------------
    (
        next = {
            $item:item
            $($rest:tt)*
        }
        tele = {$(($tele:ident: $($tele_type:tt)*))*}
    ) => {
        // emit same item
        $item

        // recursive call:
        $crate::macros::better_modules! {
            next = {$($rest)*}
            tele = {$(($tele: $($tele_type)*))*}
        }
    },

    // ----------------------
    // case n: empty
    // ----------------------
    (
        next = {}
        tele = {$(($tele:ident: $($tele_type:tt)*))*}
    ) => {},

    // ----------------------
    // entry point: convert user-facing format to internal format
    // ----------------------
    (
        mod $(($tele:ident: $($tele_type:tt)*))* {
            $($content:tt)*
        }
    ) => {
        $crate::macros::better_modules! {
            next = { $($content)* }
            tele = { $(($tele: $($tele_type)*))* }
        }
    },
}

pub trait Test {
	type MyVal;
}

better_modules! {
	mod (A: Test) (B: Test) {
		if () {
			pub struct X {
				value: A::MyVal,
			}
		} else {
			pub struct X {
				#[allow(unused)]
				field1: A::MyVal,
				#[allow(unused)]
				field2: B::MyVal,
			}

			impl sp_std::fmt::Debug for X<A,B> {
				fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
					Ok(())
				}
			}
		}
	}
}

/// Helper macro to construct an enum tuple variant from a tuple expression.
///
/// Declarative macros cannot generate numbered identifiers for destructuring,
/// so this macro uses a fixed pool of identifiers and consumes one per tuple element.
///
/// Usage:
/// ```ignore
/// tuple_into_enum_variant!(self.value; MyEnum::MyVariant; Type1, Type2, Type3)
/// ```
/// Expands to the equivalent of:
/// ```ignore
/// { let (_0, _1, _2) = self.value; MyEnum::MyVariant(_0, _1, _2) }
/// ```
#[macro_export]
macro_rules! tuple_into_enum_variant {
	// Entry point: start recursive accumulation
	($tuple:expr; $Enum:ident :: $Variant:ident; $($ty:ty),* $(,)?) => {
		$crate::tuple_into_enum_variant!(
			@acc $tuple; $Enum::$Variant;
			[] [$($ty),*];
			[_tv0 _tv1 _tv2 _tv3 _tv4 _tv5 _tv6 _tv7 _tv8 _tv9 _tv10 _tv11 _tv12 _tv13 _tv14 _tv15]
		)
	};
	// Base case: all types consumed, emit the destructure + construction
	(@acc $tuple:expr; $Enum:ident :: $Variant:ident; [$($id:ident)*] []; [$($pool:ident)*]) => {
		{
			#[allow(unused)]
			let ($($id,)*) = $tuple;
			$Enum::$Variant($($id),*)
		}
	};
	// Recursive case: consume one type, take one identifier from the pool
	(@acc $tuple:expr; $Enum:ident :: $Variant:ident; [$($id:ident)*] [$_ty:ty $(, $rest:ty)*]; [$next:ident $($pool:ident)*]) => {
		$crate::tuple_into_enum_variant!(
			@acc $tuple; $Enum::$Variant;
			[$($id)* $next] [$($rest),*];
			[$($pool)*]
		)
	};
}
pub use tuple_into_enum_variant;

/// Helper macro to call another macro with a comma-separated list of fresh identifiers, one for
/// each type.
///
/// Declarative macros cannot generate numbered identifiers, so this macro uses a fixed pool of
/// identifiers and consumes one per provided type. Because a `macro_rules!` invocation cannot
/// reliably expand to a bare comma-separated fragment in pattern/expression contexts, the generated
/// identifiers are passed to a callback macro.
///
/// Usage:
/// ```text
/// comma_separated_identifiers_for!(some_callback; Type1, Type2, Type3)
/// ```
/// Expands to the equivalent of:
/// ```text
/// some_callback!(_tv0, _tv1, _tv2)
/// ```
#[macro_export]
macro_rules! comma_separated_identifiers_for {
    ($callback:ident; $($ty:ty),* $(,)?) => {
        $crate::comma_separated_identifiers_for!(
            @acc
            $callback;
            [] [$($ty),*];
            [_tv0 _tv1 _tv2 _tv3 _tv4 _tv5 _tv6 _tv7 _tv8 _tv9 _tv10 _tv11 _tv12 _tv13 _tv14 _tv15]
        )
    };
    (@acc $callback:ident; [$($id:ident)*] []; [$($pool:ident)*]) => {
        $callback!($($id),*)
    };
    (@acc $callback:ident; [$($id:ident)*] [$_ty:ty $(, $rest:ty)*]; [$next:ident $($pool:ident)*]) => {
        $crate::comma_separated_identifiers_for!(
            @acc
            $callback;
            [$($id)* $next] [$($rest),*];
            [$($pool)*]
        )
    };
}
pub use comma_separated_identifiers_for;
