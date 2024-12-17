use jsonrpsee_flatten::types::ArrayParam;
use schemars::{json_schema, JsonSchema};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

pub mod register_account;
pub mod request_swap_deposit_address;
pub mod request_swap_parameter_encoding;
pub mod withdraw_fees;

api_json_schema::impl_schema_endpoint! {
	prefix: "broker_",
	RequestSwapDepositAddress: request_swap_deposit_address::Endpoint,
	RegisterAccount: register_account::Endpoint,
	RequestSwapParameterEncoding: request_swap_parameter_encoding::Endpoint,
	WithdrawFees: withdraw_fees::Endpoint,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MockApi;

/// The empty type's [ArrayParam] implementation needs to use `[(); 0]` as the 'empty array' type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Empty;

impl JsonSchema for Empty {
	fn schema_name() -> std::borrow::Cow<'static, str> {
		"Empty".into()
	}

	fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
		json_schema!({
			"oneOf": [
				{
					"description": "The empty Array `[]`.",
					"type": "array",
					"const": "[]"
				},
				{
					"description": "The empty object `{}`.",
					"type": "object",
					"const": "{}"
				}
			]
		})
	}
}

impl ArrayParam for Empty {
	type ArrayTuple = [(); 0];

	fn into_array_tuple(self) -> Self::ArrayTuple {
		[(); 0]
	}

	fn from_array_tuple(_: Self::ArrayTuple) -> Self {
		Empty
	}
}
