use std::{error::Error as StdError, sync::Arc};

use jsonrpsee::core::params::ArrayParams;

use crate::types::JsValue;

pub trait Call: Send + Sync {
	type Response: serde::de::DeserializeOwned + Send;

	const CALL_METHOD_NAME: &'static str;
	fn call_params(&self) -> ArrayParams;

	fn process_response(&self, input: JsValue) -> Result<Self::Response, serde_json::Error> {
		serde_json::from_value(input)
	}
}

#[async_trait::async_trait]
pub trait CallApi: Send + Sync {
	type Error: StdError + Send + Sync + 'static;
	async fn call<C: Call>(&self, call: C) -> Result<C::Response, Self::Error>;
}

impl<'a, C> Call for &'a C
where
	C: Call,
{
	type Response = C::Response;

	const CALL_METHOD_NAME: &'static str = C::CALL_METHOD_NAME;
	fn call_params(&self) -> ArrayParams {
		<C as Call>::call_params(*self)
	}
}

#[async_trait::async_trait]
impl<'a, A> CallApi for &'a A
where
	A: CallApi,
{
	type Error = A::Error;

	async fn call<C: Call>(&self, call: C) -> Result<C::Response, Self::Error> {
		<A as CallApi>::call(*self, call).await
	}
}

#[async_trait::async_trait]
impl<A> CallApi for Box<A>
where
	A: CallApi,
{
	type Error = A::Error;

	async fn call<C: Call>(&self, call: C) -> Result<C::Response, Self::Error> {
		<A as CallApi>::call(self.as_ref(), call).await
	}
}

#[async_trait::async_trait]
impl<A> CallApi for Arc<A>
where
	A: CallApi,
{
	type Error = A::Error;

	async fn call<C: Call>(&self, call: C) -> Result<C::Response, Self::Error> {
		<A as CallApi>::call(self.as_ref(), call).await
	}
}
