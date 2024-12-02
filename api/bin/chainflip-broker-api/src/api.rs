use jsonrpsee_flatten::types::ArrayParam;
use schemars::JsonSchema;
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
	Schema: schema::Endpoint,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MockApi;

pub struct Empty;
impl ArrayParam for Empty {
	type ArrayTuple = ((),);

	fn into_array_tuple(self) -> Self::ArrayTuple {
		((),)
	}

	fn from_array_tuple(((),): Self::ArrayTuple) -> Self {
		Empty
	}
}
