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
