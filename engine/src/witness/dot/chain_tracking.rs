use cf_chains::dot::{PolkadotHash, PolkadotTrackedData};
use subxt::events::Phase;

use crate::{dot::retry_rpc::DotRetryRpcApi, witness::dot::EventWrapper};

use super::super::common::{
	chain_source::Header, chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData,
};

#[async_trait::async_trait]
impl<T: DotRetryRpcApi + Send + Sync + Clone>
	GetTrackedData<cf_chains::Polkadot, PolkadotHash, Vec<(Phase, EventWrapper)>> for T
{
	async fn get_tracked_data(
		&self,
		header: &Header<
			<cf_chains::Polkadot as cf_chains::Chain>::ChainBlockNumber,
			PolkadotHash,
			Vec<(Phase, EventWrapper)>,
		>,
	) -> Result<<cf_chains::Polkadot as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let events = &header.data;

		let mut tips = Vec::new();
		for (phase, wrapped_event) in events.iter() {
			if let Phase::ApplyExtrinsic(_) = phase {
				if let EventWrapper::TransactionFeePaid { tip, .. } = wrapped_event {
					tips.push(*tip);
				}
			}
		}

		Ok(PolkadotTrackedData {
			median_tip: {
				tips.sort();
				tips.get(tips.len().saturating_sub(1) / 2).cloned().unwrap_or_default()
			},
			runtime_version: self.runtime_version(None).await,
		})
	}
}
