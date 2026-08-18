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

use quote::quote;

/// The span is reached through `cf_runtime_utilities`, which owns the `sp-tracing` dependency, so
/// that annotating a function requires nothing of the crate it lives in.
pub fn expand(
	attr: proc_macro2::TokenStream,
	mut function: syn::ItemFn,
) -> proc_macro2::TokenStream {
	let span_name = function.sig.ident.to_string();

	let fields = if attr.is_empty() {
		quote!()
	} else if syn::parse2::<syn::Ident>(attr.clone()).is_ok_and(|ident| ident == "pallet") {
		quote!(, pallet = <Self as
			::cf_runtime_utilities::__reexports::frame_support::traits::PalletInfoAccess>::name())
	} else {
		quote!(, #attr)
	};

	function.block.stmts.insert(
		0,
		syn::parse_quote! {
			::cf_runtime_utilities::__reexports::sp_tracing::enter_span!(
				::cf_runtime_utilities::__reexports::sp_tracing::trace_span!(#span_name #fields)
			);
		},
	);
	quote!(#function)
}
