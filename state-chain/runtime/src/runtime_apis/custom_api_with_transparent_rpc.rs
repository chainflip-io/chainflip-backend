use crate::{decl_runtime_apis_with_transparent_rpc, runtime_apis::transparent_rpc_generator::{runtime_and_rpc_layer::BrokerInfo, type_variants::{AtRuntime, V2_1, V2_2}}};




// define_empty_struct! {
// 	pub struct Test;
// }
// define_empty_struct! {
// 	pub struct TestRpc;
// }

// impl HasVariant<AtRpcLayer> for Test {
// 	type Get = TestRpc;

// 	fn from_variant(at_version: Self::Get) -> Self {
// 		Test {}
// 	}

// 	fn to_variant(self) -> Self::Get {
// 		TestRpc {}
// 	}
// }



decl_runtime_apis_with_transparent_rpc! {
	versions {
		1 => V2_1,
		2 => V2_2,
	}

	trait macro generate_transparent_custom_rpc;
	impl macro generate_versioned_product_custom_rpc_impl;

	#[api_version(2)]
	trait TransparentCustomRuntimeApi {
		#[method(name = "mytest")]
		#[changed_in()]
		fn mytest(arg:()) -> ();

		#[method(name = "mytest2")]
		#[changed_in()]
		fn mytest2(arg: BrokerInfo::Struct<AtRuntime>) -> BrokerInfo::Struct<AtRuntime>;
	}
}




