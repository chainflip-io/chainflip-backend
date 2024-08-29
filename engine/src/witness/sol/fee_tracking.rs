use crate::sol::{
	retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
	rpc_client_api::RpcPrioritizationFee,
};

pub async fn get_median_prioritization_fee(sol_client: &SolRetryRpcClient) -> Option<u64> {
	let fees = sol_client.get_recent_prioritization_fees().await;

	if fees.is_empty() {
		None
	} else {
		let mut fees = fees
			.into_iter()
			.map(|RpcPrioritizationFee { prioritization_fee, .. }| prioritization_fee)
			.collect::<Vec<_>>();
		let median_index = fees.len().saturating_sub(1) / 2;
		// Note `select_nth_unstable` panics on an empty slice, but we've already handled that
		// case.
		let (_, median, _) = fees.select_nth_unstable(median_index);
		Some(*median)
	}
}
