use crate::{sol::retry_rpc::SolRetryRpcApi, witness::common::chain_source::Header};

use cf_chains::sol::SolTrackedData;

use super::super::common::chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData;
use utilities::context;

use cf_chains::sol::SolHash;

#[async_trait::async_trait]
impl<T: SolRetryRpcApi + Send + Sync + Clone> GetTrackedData<cf_chains::Solana, SolHash, ()> for T {
	async fn get_tracked_data(
		&self,
		_header: &Header<<cf_chains::Solana as cf_chains::Chain>::ChainBlockNumber, SolHash, ()>,
	) -> Result<<cf_chains::Solana as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let priorization_fees = self.get_recent_prioritization_fees().await;

		let mut priority_fees: Vec<u64> =
			priorization_fees.iter().map(|f| f.prioritization_fee).collect();
		priority_fees.sort();

		// These fees won't be consistent accross CFEs so we are handling that in the runtime.
		Ok(SolTrackedData {
			priority_fee: (context!(priority_fees
				.get(priority_fees.len().saturating_sub(1) / 2)
				.cloned())?),
		})
	}
}
