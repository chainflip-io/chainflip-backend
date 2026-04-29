

use cf_rpc_apis::NumberOrHex;
use jsonrpsee::{
	core::async_trait,
	proc_macros::rpc,
};
use state_chain_runtime::runtime_apis::transparent_rpc_generator::type_variants::RpcToRuntime;

// use state_chain_runtime::{generate_transparent_custom_rpc, runtime_apis::custom_api_with_transparent_rpc::{
// 	AtRpc, HasVariant, TransparentCustomApi,
// }};
use crate::BrokerInfo;
use state_chain_runtime::runtime_apis::transparent_rpc_generator::type_variants::AtRuntime;
use state_chain_runtime::runtime_apis::transparent_rpc_generator::runtime_and_rpc_layer::PrimitiveTypes;
use state_chain_runtime::runtime_apis::transparent_rpc_generator::type_variants::AtRpc;
use state_chain_runtime::runtime_apis::transparent_rpc_generator::type_variants::TypedMigration;

// from super module
use crate::CustomRpc;


generate_transparent_custom_rpc! {
	#[rpc(server, client, namespace = "cf_experimental")]
	trait TransparentCustomApi where {
		server = trait TransparentCustomApiServer,
		server = struct CustomRpc,
		client = trait TransparentCustomApiClient,
	}
}

// ------------ definition of primitive types at rpc layer -----------

impl PrimitiveTypes for AtRpc {
	type AssetAmount = u64;
	type BtcAddress = u16;
	type AccountId = u16;
}

// ------------ migrations for all primitives types ----------


/// The migration between rpc and runtime layer
pub struct RpcToRuntime;

impl<X: HasVariant<AtRpc>> HasMigrationFrom<AtRpc> for X
where RpcToRuntime: TypedMigration<X::Get, X>
{
    type GetMigration = FromTypedMigration<X::Get, X, RpcToRuntime>;
}

impl TypedMigration<cf_primitives::AssetAmount, NumberOrHex> for RpcToRuntime {
    fn forwards(x: cf_primitives::AssetAmount) -> NumberOrHex {
        todo!()
    }

    fn backwards(x: NumberOrHex) -> cf_primitives::AssetAmount {
        todo!()
    }
}