use crate::sol::{
	retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
	rpc_client_api::RpcPrioritizationFee,
};

pub async fn get_median_prioritization_fee(sol_client: &SolRetryRpcClient) -> Option<u64> {
	let mut fees = sol_client
		.get_recent_prioritization_fees()
		.await
		.into_iter()
		.map(|RpcPrioritizationFee { prioritization_fee, .. }| prioritization_fee)
		.collect::<Vec<_>>();

	if fees.is_empty() {
		// No fees were paid, we should not need to pay any either.
		None
	} else {
		let median_index = fees.len().saturating_sub(1) / 2;
		// Note `select_nth_unstable` panics on an empty slice, but we've already handled that
		// case.
		let (_, median, _) = fees.select_nth_unstable(median_index);
		Some(*median)
	}
}
