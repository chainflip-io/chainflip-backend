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

use proc_macro::TokenStream;

mod arbitrary;
mod generate_module;
mod generic_modules;
mod intro_elim;
mod type_introspection;

/// Proc macro for writing groups of local items under a shared type-parameter
/// telescope without repeating those parameters at every local reference.
///
/// A telescope scope is written as `mod (A: Bound) (B) { ... }`. It is not emitted
/// as a Rust module. Instead, its type parameters are threaded into generated
/// structs, traits, impls, and aliases inside the scope. Telescope scopes can
/// appear anywhere in the macro input, can be empty (`mod { ... }`), and can be
/// nested. Nested scopes inherit the outer telescope parameters and append their
/// own.
///
/// Local definitions are tracked by scope. When a later local path refers to one
/// of them, `generic_modules!` adds the definition's telescope arguments for you:
///
/// ```ignore
/// generic_modules! {
///     mod (A: Trait) {
///         pub struct Local { value: A::Value }
///         pub type Alias = Local; // expands as Local<A>
///     }
/// }
/// ```
///
/// If a local definition is referenced outside the telescope that introduced one
/// of its parameters, that parameter is not in scope anymore. In that case an
/// explicitly supplied generic argument is used for the out-of-scope telescope
/// parameter instead:
///
/// ```ignore
/// generic_modules! {
///     mod (A: Trait) {
///         pub struct Local { value: A::Value }
///     }
///
///     pub type Concrete<X: Trait> = Local<X>;
/// }
/// ```
///
/// Telescope scopes can also carry additional where predicates:
/// `mod (A: Trait) where (A::Value: Clone) { ... }`. Each predicate is wrapped in
/// parentheses, matching the type-parameter syntax. These predicates are added to
/// generated structs, traits, and impls, preserving any user-written where
/// clauses. For type aliases, telescope where predicates are added only when the
/// alias actually inherits one of the telescope parameters mentioned by the
/// predicate.
///
/// The macro also supports local `use` imports, nested real Rust modules,
/// expression paths in impl bodies, trait bounds, and lazy type aliases with a
/// `where` clause after the aliased type.
///
/// Usage:
/// ```ignore
/// generic_modules! {
///     pub type Plain = u8;
///
///     mod (A: Trait) (B: Trait) where (A::Assoc: Clone) (B: Copy) {
///         pub type Alias = (A::Assoc, B::Assoc);
///
///         mod (C: OtherTrait) {
///             pub type Nested = (Alias, C::Assoc);
///         }
///
///         pub struct Foo {
///             field: A::Assoc,
///         }
///
///         impl SomeTrait for Foo {
///             // ...
///         }
///
///         if (condition) {
///             // emitted when condition is non-empty
///         } else {
///             // emitted when condition is empty
///         }
///     }
/// }
/// ```
#[proc_macro]
pub fn generic_modules(input: TokenStream) -> TokenStream {
	let input = syn::parse_macro_input!(input as generic_modules::Input);
	generic_modules::expand(input).into()
}

/// Attribute macro that wraps the annotated struct in a `cf_utilities::generate_module!`
/// invocation, automatically generating the module name as `_StructName`.
///
/// Usage:
/// ```ignore
/// #[generate_module]
/// struct MyStruct {
///     field: Type,
/// }
/// ```
///
/// Expands to:
/// ```ignore
/// cf_utilities::generate_module! {
///     struct MyStruct {
///         field: Type,
///     }
///     mod _MyStruct { #![migrations] }
/// }
/// ```
#[proc_macro_attribute]
pub fn generate_module(_attr: TokenStream, item: TokenStream) -> TokenStream {
	let item_clone: proc_macro2::TokenStream = item.clone().into();
	let parsed = syn::parse_macro_input!(item as syn::Item);
	generate_module::expand(item_clone, parsed).into()
}

/// Derive macro that implements `cf_utilities::type_introspection::HasTypeIntrospection`.
///
/// The trait exposes two pieces of structural information:
///
/// - `is_empty_type()` returns whether the type has no constructible values.
/// - `sample_all_shapes()` returns example values that enumerate every possible structural shape.
///
/// For structs, a type is empty when any field type is empty. Shape samples are built from the full
/// Cartesian product of every field's samples.
///
/// For enums, a type is empty only when all variants are empty. Shape samples are built by sampling
/// each variant independently and concatenating those variant samples. Unit variants contribute one
/// sample, while variants containing an empty field contribute none.
///
/// Unions are not supported.
///
/// ## Example
///
/// ```ignore
/// #[derive(cf_proc_macros::HasTypeIntrospection)]
/// enum Value {
///     Empty(Never),
///     One(u8),
///     Pair { left: bool, right: Option<u8> },
/// }
/// ```
#[proc_macro_derive(HasTypeIntrospection)]
pub fn derive_has_type_introspection(input: TokenStream) -> TokenStream {
	let input = syn::parse_macro_input!(input as syn::DeriveInput);
	type_introspection::derive(input).into()
}

/// Derive macro that adds simple constructor/destructor helpers.
///
/// For structs with named fields, this generates an `intro(...) -> Self` constructor with one
/// argument per field. Tuple structs and unit structs are not supported.
///
/// For enums, this generates an `elim(...) -> Output` method. The method consumes `self` and takes
/// one handler closure per variant. Each handler receives the fields of its corresponding variant.
/// Unit variants receive a zero-argument handler.
///
/// ## Examples
///
/// ```ignore
/// #[derive(cf_proc_macros::IntroElim)]
/// struct Pair {
///     left: u8,
///     right: u16,
/// }
///
/// let pair = Pair::intro(1, 2);
/// ```
///
/// ```ignore
/// #[derive(cf_proc_macros::IntroElim)]
/// enum Value {
///     None,
///     One(u8),
///     Pair { left: u8, right: u16 },
/// }
///
/// let output = value.elim(
///     || 0,
///     |one| one as u16,
///     |left, right| left as u16 + right,
/// );
/// ```
#[proc_macro_derive(IntroElim)]
pub fn derive_intro_elim(input: TokenStream) -> TokenStream {
	let input = syn::parse_macro_input!(input as syn::DeriveInput);
	intro_elim::derive(input).into()
}

/// Derive macro that implements `proptest::arbitrary::Arbitrary` for a struct.
///
/// Handles structs with any number of fields (not limited to 12) by chunking
/// fields into nested strategy tuples. `PhantomData` fields are automatically
/// filled with `Default::default()`.
///
/// ## Attributes
///
/// - `#[arbitrary(bound = "T: Trait, U: OtherTrait")]` — Override the default where-clause bounds
///   on the generated impl. When not specified, each non-phantom field type gets an `Arbitrary +
///   'static` bound and each type parameter gets `'static`.
///
/// ## Example
///
/// ```ignore
/// #[derive(cf_proc_macros::ArbitraryWithBounds)]
/// #[arbitrary(bound = "Ty: 'static, Ty::amount: Arbitrary + 'static")]
/// pub struct MyStruct<Ty: Types> {
///     pub amount: Ty::amount,
///     pub count: Ty::count,
///     _phantom: PhantomData<(Ty,)>,
/// }
/// ```
#[proc_macro_derive(ArbitraryWithBounds, attributes(arbitrary))]
pub fn derive_arbitrary_with_bounds(input: TokenStream) -> TokenStream {
	let input = syn::parse_macro_input!(input as syn::DeriveInput);
	arbitrary::derive(input).into()
}
