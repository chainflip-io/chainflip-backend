pub use jsonrpsee_flatten::core::async_trait;
use schemars::{JsonSchema, Schema};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt::{Debug, Display};

mod schema_macro;

pub mod register_account;
pub mod request_swap_deposit_address;
pub mod request_swap_parameter_encoding;
pub mod withdraw_fees;

crate::impl_schema_endpoint! {
	request_swap_deposit_address: RequestSwapDepositAddress,
	register_account: RegisterAccount,
	request_swap_parameter_encoding: RequestSwapParameterEncoding,
	withdraw_fees: WithdrawFees,
}

// TODO: move the rest of this and the schema endpoint macro into a shared crate so it can be used
// by lp api etc.

/// An endpoint is a request-response pair that can be implemented by a [Responder].
pub trait Endpoint {
	type Request: JsonSchema + Serialize + DeserializeOwned;
	type Response: JsonSchema + Serialize + DeserializeOwned;
	type Error: Debug + Display;
}

/// An API extension trait for defining how an endpoint should respond to a request.
#[async_trait]
pub trait Responder<E: Endpoint> {
	async fn respond(&self, request: E::Request) -> EndpointResult<E>;
}

pub async fn respond<T: Responder<E>, E: Endpoint>(
	responder: T,
	request: E::Request,
) -> EndpointResult<E> {
	responder.respond(request).await
}

pub type EndpointRequest<E> = <E as Endpoint>::Request;
pub type EndpointResponse<E> = <E as Endpoint>::Response;
pub type EndpointError<E> = <E as Endpoint>::Error;
pub type EndpointResult<E> = Result<EndpointResponse<E>, EndpointError<E>>;

#[derive(Debug)]
pub enum Never {}
impl std::error::Error for Never {}
impl std::fmt::Display for Never {
	fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		unreachable!()
	}
}
