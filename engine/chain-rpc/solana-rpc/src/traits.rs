use crate::{
	error::Error,
	responses::LatestBlockhash,
	types::{Commitment, JsValue, PrioritizationFeeRecord, Response},
};

#[async_trait::async_trait]
pub trait SolanaGetLatestBlockhash {
	async fn get_latest_blockhash(
		&self,
		commitment: Commitment,
	) -> Result<Response<LatestBlockhash>, Error>;
}

#[async_trait::async_trait]
pub trait SolanaGetFeeForMessage {
	async fn get_fee_for_message<M>(
		&self,
		message: M,
		commitment: Commitment,
	) -> Result<Response<Option<u64>>, Error>
	where
		M: AsRef<[u8]> + Send;
}

#[async_trait::async_trait]
pub trait SolanaGetRecentPrioritizationFees {
	async fn get_recent_prioritization_fees<I>(
		&self,
		accounts: I,
	) -> Result<Vec<PrioritizationFeeRecord>, Error>
	where
		I: IntoIterator<Item = [u8; crate::types::ACCOUNT_ADDRESS_LEN]> + Send;
}

blanket_impl!(
	SolanaCallApi,
	SolanaGetLatestBlockhash,
	SolanaGetFeeForMessage,
	SolanaGetRecentPrioritizationFees
);
blanket_impl!(SolanaApi, SolanaCallApi);
