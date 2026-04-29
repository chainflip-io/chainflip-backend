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

use super::types::*;
use sp_api::decl_runtime_apis;

// ------------------ versions ---------------------

trait Version {}
struct V2_2; impl Version for V2_2 {}
struct V2_1; impl Version for V2_1 {}
struct V2_0; impl Version for V2_0 {}

trait AtVersion<V: Version> {
	type Get;
}

// ------------------ migrations --------------------


struct IdentityMigration<X>(sp_std::marker::PhantomData<X>);

impl<X> Migration for IdentityMigration<X> {
	type From = X;
	type To = X;
}

trait Migration {
	type From;
	type To;
}

trait Versioned {
	type Version_2_0;
	type Version_2_1;
}

type Source<M: Migration> = M::From;
type Target<M: Migration> = M::To;

trait Migrations: Sized
{
	type To_V_2_0: Migration<To = Source<Self::To_V_2_1>> = IdentityMigration<Source<Self::To_V_2_1>>;
	type To_V_2_1: Migration<To = Source<Self::To_V_2_2>> = IdentityMigration<Source<Self::To_V_2_2>>;
	type To_V_2_2: Migration<To = Self> = IdentityMigration<Self>;
}

// impl<X: Migrations> AtVersion<V2_1> for X {
// 	type Get = <X::To_V_2_2 as Migration>::From;
// }

// impl<X: Migrations> AtVersion<V2_2> for X {
// 	type Get = X;
// }

impl<V: Version> AtVersion<V> for () {
	type Get = ();
}

impl<V: Version, A: AtVersion<V>> AtVersion<V> for (A,) {
	type Get = (A::Get,);
}

trait ApiVersion<const N: usize> {
	type AsVersion: Version;
}

macro_rules! expand_changed_fns {
	(
		#[changed_in($($changed_version: literal),*)]
		fn $fn_name:ident($arg_name:ident : $arg_ty:ty) -> $result_ty:ty;
	) => {
		$(
			#[changed_in($changed_version)]
			fn $fn_name(arg : AtApiVersion<$changed_version, $arg_ty>) -> AtApiVersion<$changed_version, $result_ty>;
		)*
	}
}


macro_rules! decl_versioned_runtime_api_and_rpc {
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
		$(
			impl ApiVersion<$api_version> for () {
				type AsVersion = $runtime_version_ty;
			}
		)*
		type AtApiVersion<const N: usize, X> = <X as AtVersion< <() as ApiVersion<N>>::AsVersion >>::Get;

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
								$arg_name: $arg_ty,
							)*
							at: Option<state_chain_runtime::Hash>
						) -> cf_rpc_apis::RpcResult<$result_ty>;
					)*
				}

				#[async_trait]
				impl<C, B, BE> $$rpc_server_trait_name for $$rpc_server_struct_name<C, B, BE>
				where
					B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
					B::Header: Unpin,
					BE: Backend<B> + Send + Sync + 'static,
					C: sp_api::ProvideRuntimeApi<B>
						+ Send
						+ Sync
						+ 'static
						+ BlockBackend<B>
						+ ExecutorProvider<B>
						+ HeaderBackend<B>
						+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
						+ BlockchainEvents<B>
						+ CallApiAt<B>
						+ StorageProvider<B, BE>,
					C::Api: $api_name<B> + ElectoralRuntimeApi<B>,
				{
					$(
						fn $fn_name(
							&self,
							$(
								$arg_name: $arg_ty,
							)*
							at: Option<state_chain_runtime::Hash>
						) -> cf_rpc_apis::RpcResult<$result_ty> {
							self.rpc_backend.with_versioned_runtime_api(
								at, |api, hash, _version| api.$fn_name(hash, ($($arg_name,)*))
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
		decl_runtime_apis!(
			#[api_version($current_api_version)]
			pub trait VersionedProductRuntimeApi {
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


// decl_runtime_apis!(
// 	#[api_version(1)]
// 	pub trait VersionedProductRuntimeApi {
// 		fn mytest() -> ();
// 	}
// );

decl_versioned_runtime_api_and_rpc!{
    versions {
        1 => V2_1,
		2 => V2_2,
    }

	trait macro generate_versioned_product_custom_rpc_trait;
	impl macro generate_versioned_product_custom_rpc_impl;

    #[api_version(2)]
    trait VersionedProductRuntimeApi {
		#[method(name = "mytest")]
		#[changed_in(1,2)]
		fn mytest(arg:()) -> ();
    }
}

decl_runtime_apis!(
	/// Versioning of runtime apis is explained here:
	/// https://docs.rs/sp-api/latest/sp_api/macro.decl_runtime_apis.html
	/// Of course it doesn't explain everything, e.g. there's a very useful
	/// `#[renamed($OLD_NAME, $VERSION)]` attribute which will handle renaming
	/// of apis automatically.
	#[api_version(2)]
	pub trait VersionedProductRuntimeApi2 {
		/// Returns SCALE encoded `Option<ElectoralDataFor<state_chain_runtime::Runtime,
		/// Instance>>`
		#[renamed("cf_electoral_data", 2)]
		fn cf_solana_electoral_data(account_id: AccountId) -> Vec<u8>;

		/// Returns SCALE encoded `BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as
		/// pallet_cf_elections::Config<Instance>>::ElectoralSystem>>`
		#[renamed("cf_filter_votes", 2)]
		fn cf_solana_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8>;

		fn cf_bitcoin_electoral_data(account_id: AccountId) -> Vec<u8>;

		fn cf_bitcoin_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8>;

		fn cf_generic_electoral_data(account_id: AccountId) -> Vec<u8>;

		fn cf_generic_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8>;

		fn cf_ethereum_electoral_data(account_id: AccountId) -> Vec<u8>;

		fn cf_ethereum_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8>;

		fn cf_arbitrum_electoral_data(account_id: AccountId) -> Vec<u8>;

		fn cf_arbitrum_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8>;
	}
);
