use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
	rpc_client_api::{
		RpcTransactionConfig, TransactionConfirmationStatus, TransactionStatus,
		UiTransactionEncoding,
	},
};
use anyhow::Result;
use cf_chains::sol::{SolSignature, LAMPORTS_PER_SIGNATURE};
use itertools::Itertools;

pub async fn get_finalized_fee(
	sol_client: &SolRetryRpcClient,
	signature: SolSignature,
) -> Result<u64> {
	match sol_client
		.get_signature_statuses(&[signature], false)
		.await
		.value
		.iter()
		.exactly_one()
		.expect("We queried for exactly one signature.")
	{
		Some(TransactionStatus {
			confirmation_status: Some(TransactionConfirmationStatus::Finalized),
			..
		}) => {
			let transaction_meta = sol_client
				.get_transaction(
					&signature,
					RpcTransactionConfig {
						encoding: Some(UiTransactionEncoding::Json),
						// Using finalized there could be a race condition where this
						// doesn't get the tx. But "Processed" is timing out so we better
						// retry with finalized.
						commitment: Some(CommitmentConfig::finalized()),
						// Using 0 as max_supported_transaction_version to ensure we query all
						// transactions regardless of version. This is not strictly necessary, but
						// ensures we don't miss anything.
						max_supported_transaction_version: Some(0),
					},
				)
				.await
				.transaction
				.meta;

			Ok(match transaction_meta {
				Some(meta) => meta.fee,
				// This shouldn't happen. Want to avoid Erroring. We either default to
				// 5000 or return OK(()) so we don't submit transaction_succeeded and
				// retry again later. Defaulting to avoid potentially getting stuck not
				// witness something because no meta is returned.
				None => LAMPORTS_PER_SIGNATURE,
			})
		},
		Some(TransactionStatus { confirmation_status: other_status, .. }) => Err(anyhow::anyhow!(
			"Transaction status is {:?}, waiting for {:?}.",
			other_status,
			TransactionConfirmationStatus::Finalized
		)),
		// TODO: Consider distinguishing this case as `Ok(None)` to indicate that the
		// request returned a response, but the tx is not available yet.
		None => Err(anyhow::anyhow!("Unknown Transaction.")),
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{NodeContainer, WsHttpEndpoints},
		sol::retry_rpc::SolRetryRpcClient,
	};

	use cf_chains::{sol::SolSignature, Chain, Solana};
	use futures_util::FutureExt;
	use std::str::FromStr;
	use utilities::task_scope;

	use super::*;

	#[tokio::test]
	#[ignore]
	async fn test_egress_witnessing() {
		task_scope::task_scope(|scope| {
			async {
				let client= SolRetryRpcClient::new(
						scope,
						NodeContainer {
							primary: WsHttpEndpoints {
								ws_endpoint: "wss://api.devnet.solana.com".into(),
								http_endpoint: "https://api.devnet.solana.com".into(),
							},
							backup: None,
						},
						None,
						Solana::WITNESS_PERIOD,
					)
					.await
					.unwrap();

				let monitored_tx_signature =
					SolSignature::from_str(
						"4udChXyRXrqBxUTr9F3nbTcPyvteLJtFQ3wM35J53NdP4GWwUp2wBwdTJEYs2aiNz7DyCqitok6ci7qqHPkRByb2").unwrap();

				let fee = get_finalized_fee(&client, monitored_tx_signature).await.unwrap();

				println!("{:?}", fee);
				assert_eq!(fee, LAMPORTS_PER_SIGNATURE);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}