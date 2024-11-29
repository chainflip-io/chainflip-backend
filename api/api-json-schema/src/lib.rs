pub mod schema_macro;

use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::{Debug, Display};

/// An endpoint is a request-response pair that can be implemented by a [Responder].
pub trait Endpoint {
	type Request: JsonSchema + Serialize + DeserializeOwned;
	type Response: JsonSchema + Serialize + DeserializeOwned;
	type Error: Debug + Display;
}

/// An API extension trait for defining how an endpoint should respond to a request.
pub trait Responder<E: Endpoint> {
	#[allow(async_fn_in_trait)]
	async fn respond(&self, request: E::Request) -> EndpointResult<E>;
}

/// Convenience function for responding to an endpoint request.
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

/// An error type that can never occur.
#[derive(Debug)]
pub enum Never {}
impl std::error::Error for Never {}
impl std::fmt::Display for Never {
	fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		unreachable!()
	}
}
