use jsonrpsee::{
	core::{client::ClientT, Error},
	http_client::HttpClient,
};

use crate::traits::{Call, CallApi};

#[async_trait::async_trait]
impl CallApi for HttpClient {
	type Error = Error;
	async fn call<C: Call>(&self, call: C) -> Result<C::Response, Self::Error> {
		self.request(C::CALL_METHOD_NAME, call.call_params())
			.await
			.and_then(|js_value| <C as Call>::process_response(&call, js_value).map_err(Into::into))
	}
}
