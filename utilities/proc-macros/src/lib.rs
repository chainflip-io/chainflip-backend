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

mod better_modules;
mod enum_elim;
mod type_introspection;

/// Proc macro that takes a "telescope" of type parameters and threads them
/// through items in telescope scopes. Telescope scopes can appear anywhere in
/// the macro input and can be nested.
///
/// Usage:
/// ```ignore
/// better_modules! {
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
pub fn better_modules(input: TokenStream) -> TokenStream {
	let input = syn::parse_macro_input!(input as better_modules::Input);
	better_modules::expand(input).into()
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
	let parsed: syn::ItemStruct =
		syn::parse(item).expect("#[generate_module] can only be applied to structs");
	let mod_name = quote::format_ident!("_{}", parsed.ident);
	let output = quote::quote! {
		cf_utilities::generate_module! {
			#item_clone
			mod #mod_name { #![migrations] }
		}
	};
	output.into()
}

#[proc_macro_derive(HasTypeIntrospection)]
pub fn derive_has_type_introspection(input: TokenStream) -> TokenStream {
	let input = syn::parse_macro_input!(input as syn::DeriveInput);
	type_introspection::derive(input).into()
}

#[proc_macro_derive(EnumElim)]
pub fn derive_enum_elim(input: TokenStream) -> TokenStream {
	let input = syn::parse_macro_input!(input as syn::DeriveInput);
	enum_elim::derive(input).into()
}
