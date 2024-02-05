use base64::Engine;
use jsonrpsee::{core::client::ClientT, http_client::HttpClient, rpc_params};
use serde_json::json;

use crate::{
	error::Error,
	responses::LatestBlockhash,
	types::{Commitment, JsValue, PrioritizationFeeRecord, Response},
};

#[async_trait::async_trait]
impl crate::traits::SolanaGetLatestBlockhash for HttpClient {
	async fn get_latest_blockhash(
		&self,
		commitment: Commitment,
	) -> Result<Response<LatestBlockhash>, Error> {
		self.request(
			"getLatestBlockhash",
			rpc_params![json!({
				"commitment": commitment,
			})],
		)
		.await
		.map_err(Error::transport)
	}
}

#[async_trait::async_trait]
impl crate::traits::SolanaGetFeeForMessage for HttpClient {
	async fn get_fee_for_message<M>(
		&self,
		message: M,
		commitment: Commitment,
	) -> Result<Response<Option<u64>>, Error>
	where
		M: AsRef<[u8]> + Send,
	{
		let message_encoded = base64::engine::general_purpose::STANDARD.encode(message);

		self.request(
			"getFeeForMessage",
			rpc_params![
				message_encoded,
				json!({
					"commitment": commitment,
				})
			],
		)
		.await
		.map_err(Error::transport)
	}
}

#[async_trait::async_trait]
impl crate::traits::SolanaGetRecentPrioritizationFees for HttpClient {
	async fn get_recent_prioritization_fees<I>(
		&self,
		accounts: I,
	) -> Result<Vec<PrioritizationFeeRecord>, Error>
	where
		I: IntoIterator<Item = [u8; crate::types::ACCOUNT_ADDRESS_LEN]> + Send,
	{
		let accounts_encoded = accounts
			.into_iter()
			.map(|a| bs58::encode(a.as_ref()).into_string())
			.collect::<Vec<_>>();

		self.request("getRecentPrioritizationFees", rpc_params![accounts_encoded])
			.await
			.map_err(Error::transport)
	}
}
