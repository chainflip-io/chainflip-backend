use jsonrpsee_flatten::types::ArrayParam;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, ops::Deref};

pub mod register_account;
pub mod request_swap_deposit_address;
pub mod request_swap_parameter_encoding;
pub mod withdraw_fees;

api_json_schema::impl_schema_endpoint! {
	request_swap_deposit_address: RequestSwapDepositAddress,
	register_account: RegisterAccount,
	request_swap_parameter_encoding: RequestSwapParameterEncoding,
	withdraw_fees: WithdrawFees,
}

// This wrapper is needed to satisify rust's foreign type implementation restritions when implement
// the `Responder` trait.
pub struct ApiWrapper<T> {
	pub api: T,
}

impl<T> Deref for ApiWrapper<T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.api
	}
}

/// The empty type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
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
