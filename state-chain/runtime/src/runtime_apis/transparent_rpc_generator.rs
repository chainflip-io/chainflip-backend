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

use crate::runtime_apis::transparent_rpc_generator::type_variants::VariantName;

use super::types::*;
use cf_utilities::define_empty_struct;
use sp_api::decl_runtime_apis;

pub mod type_variants;
pub mod runtime_and_rpc_layer;
pub mod transformations;


// ------------------ migrations --------------------


// trait Migration {
// 	type From;
// 	type To;
// }

// trait Versioned {
// 	type Version_2_0;
// 	type Version_2_1;
// }




// impl<X: Migrations> HasVariant<V2_1> for X {
// 	type Get = <X::To_V_2_2 as Migration>::From;
// }

// impl<X: Migrations> HasVariant<V2_2> for X {
// 	type Get = X;
// }

// impl<V: VariantName> type_variants::HasVariant<V> for () {
// 	type Get = ();

// 	fn to_variant(self) -> Self::Get {
// 		()
// 	}

// 	fn from_variant(at_version: Self::Get) -> Self {
// 		()
// 	}
// }

// impl<V: VariantName, A: HasVariant<V>> HasVariant<V> for (A,) {
// 	type Get = (A::Get,);

// 	fn to_variant(self) -> Self::Get {
// 		(self.0.to_variant(),)
// 	}

// 	fn from_variant(at_version: Self::Get) -> Self {
// 		todo!()
// 	}
// }

pub trait ApiVersion<const N: usize> {
	type AsVersion: VariantName;
}

#[macro_export]
macro_rules! decl_runtime_apis_with_transparent_rpc {
    (
        versions {
            $(
                $api_version:literal => $runtime_version_ty:ident,
            )*
        }

		trait macro $generate_custom_rpc_trait:ident;
		impl macro $generate_custom_rpc_impl:ident;

        #[api_version($current_api_version:literal)]
        trait $api_name:ident {
            $(
				#[method(name = $rpc_name:literal)]
				#[changed_in($($changed_version:literal),*)]
                fn $fn_name:ident(
					$(
						$arg_name:ident: $arg_ty:ty
					),*
				) -> $result_ty:ty;
            )*
        }
    ) => {
		use crate::runtime_apis::transparent_rpc_generator::ApiVersion;
		use crate::runtime_apis::transparent_rpc_generator::type_variants::HasVariant;
		use crate::decl_versioned_runtime_apis;

		$(
			impl ApiVersion<$api_version> for () {
				type AsVersion = $runtime_version_ty;
			}
		)*
		type AtApiVersion<const N: usize, X> = <X as HasVariant< <() as ApiVersion<N>>::AsVersion >>::Get;

		// defining the trait for runtime apis
		decl_versioned_runtime_apis!{
			#[api_version($current_api_version)]
			trait $api_name {
				$(
					#[changed_in($($changed_version),*)]
					fn $fn_name(
						// we make a tuple of all arguments
						arg: (
							$( $arg_ty,)*
						)
					) -> $result_ty;
				)*
			}
		}

		// defining macros to inject the custom rpc declarations
		#[macro_export]
		macro_rules! $generate_custom_rpc_trait {
			(
				$$(
					#[$$($$Attributes:tt)*]
				)*
				trait $$rpc_trait_name:ident where {
					server = trait $$rpc_server_trait_name:ident,
					server = struct $$rpc_server_struct_name:ident,
					client = trait $$rpc_client_trait_name:ident,
					translation_to_runtime_types = $$migration:ident,
				}
			) => {

				$$(
					#[$$($$Attributes)*]
				)*
				trait $$rpc_trait_name {
					$(
						#[method(name = $rpc_name)]
						fn $fn_name(
							&self,
							$(
								$arg_name: GetVariant<AtRpc, $arg_ty>,
							)*
							at: Option<state_chain_runtime::Hash>
						) -> cf_rpc_apis::RpcResult<GetVariant<AtRpc, $result_ty>>;
					)*
				}

				#[async_trait]
				impl<C, B, BE> $$rpc_server_trait_name for $$rpc_server_struct_name<C, B, BE>
				where
					B: sp_runtime::traits::Block<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
					B::Header: Unpin,
					BE: sc_client_api::Backend<B> + Send + Sync + 'static,
					C: sp_api::ProvideRuntimeApi<B>
						+ sp_api::CallApiAt<B>
						+ sc_client_api::BlockBackend<B>
						+ sc_client_api::ExecutorProvider<B>
						+ sc_client_api::HeaderBackend<B>
						+ sc_client_api::StorageProvider<B, BE>
						+ sc_client_api::BlockchainEvents<B>
						+ sc_client_api::blockchain::HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
						+ Send
						+ Sync
						+ 'static,
					C::Api: $api_name<B>,
				{
					$(
						fn $fn_name(
							&self,
							$(
								$arg_name: GetVariant<AtRpc, $arg_ty>,
							)*
							at: Option<state_chain_runtime::Hash>
						) -> cf_rpc_apis::RpcResult<GetVariant<AtRpc, $result_ty>> {
							self.rpc_backend.with_versioned_runtime_api(
								at,
								|api, hash, _version| {
									// convert arguments to runtime layer types
									let runtime_args = (
										$(
											<$$migration as TypedMigration< <$arg_ty as HasVariant<AtRpc>>::Get, $arg_ty >>::forwards($arg_name),
											// <<$arg_ty as HasMigrationFrom<AtRpc>>::GetMigration as Migration>::backwards($arg_name),
										)*
									);

									// call runtime call
									let runtime_result = api.$fn_name(hash, runtime_args);

									// convert result back to rpc layer variant
									runtime_result.map(|value| 
										<$$migration as TypedMigration< <$result_ty as HasVariant<AtRpc>>::Get, $result_ty >>::backwards(value)
									)
								}
							)
						}
					)*
				}
			}
		}
		pub use $generate_custom_rpc_trait;

		// decl_runtime_apis!(
		// 	#[api_version($current_api_version)]
		// 	pub trait VersionedProductRuntimeApi {
		// 		$(
		// 			$(
		// 				#[changed_in($changed_version)]
		// 				fn $fn_name(arg : AtApiVersion<$changed_version, $arg_ty>) -> AtApiVersion<$changed_version, $result_ty>;
		// 			)*
		// 			fn $fn_name($arg_name: $arg_ty) -> $result_ty;
		// 		)*
		// 	}
		// );
    };
}
pub use decl_runtime_apis_with_transparent_rpc;

#[macro_export]
macro_rules! decl_versioned_runtime_apis {
    (
        #[api_version($current_api_version:literal)]
        trait $api_name:ident {
            $(
				#[changed_in($($changed_version:literal),*)]
                fn $fn_name:ident(
					$arg_name:ident: $arg_ty:ty
				) -> $result_ty:ty;
            )*
        }
    ) => {
		use sp_api::decl_runtime_apis;
		decl_runtime_apis!(
			#[api_version($current_api_version)]
			pub trait $api_name {
				$(
					$(
						#[changed_in($changed_version)]
						fn $fn_name(arg : AtApiVersion<$changed_version, $arg_ty>) -> AtApiVersion<$changed_version, $result_ty>;
					)*
					fn $fn_name($arg_name: $arg_ty) -> $result_ty;
				)*
			}
		);
    };
}

pub use decl_versioned_runtime_apis;
