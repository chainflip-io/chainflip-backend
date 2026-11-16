// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::sol::{
	commitment_config::CommitmentConfig,
	retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
	rpc_client_api::{
		RpcTransactionConfig, TransactionConfirmationStatus, TransactionStatus,
		UiTransactionEncoding, UiTransactionStatusMeta,
	},
};
use anyhow::Result;
use cf_chains::sol::{SolSignature, LAMPORTS_PER_SIGNATURE};
use itertools::Itertools;
use serde_json::Value;

pub async fn get_finalized_fee_and_success_status(
	sol_client: &SolRetryRpcClient,
	signature: SolSignature,
) -> Result<Option<(u64, bool)>> {
	match sol_client
		.get_signature_statuses(&[signature])
		.await
		.value
		.iter()
		.exactly_one()
		.expect("We queried for exactly one signature.")
	{
		Some(TransactionStatus {
			confirmation_status: Some(TransactionConfirmationStatus::Finalized),
			err,
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
						// Query for both Legacy and Versioned transactions since we can
						// build both types.
						max_supported_transaction_version: Some(0),
					},
				)
				.await
				.transaction
				.meta;

			Ok(fee_and_success_status(signature, err, transaction_meta.as_ref()))
		},
		Some(TransactionStatus { confirmation_status: other_status, .. }) => {
			tracing::debug!(
				"Transaction({:?}) status is {:?}, waiting for {:?}.",
				signature,
				other_status,
				TransactionConfirmationStatus::Finalized
			);
			Ok(None)
		},
		None => {
			// The request returned a response, but the the tx is not available yet.
			tracing::debug!("Unknown Transaction ({signature:?})");
			Ok(None)
		},
	}
}

/// Success is taken from the finalized signature status, which is always available once the
/// signature is finalized, whereas the transaction meta can be missing (eg. if the RPC node has not
/// caught up or has pruned it). The meta is only needed for the exact fee, and to cross-check the
/// status.
fn fee_and_success_status(
	signature: SolSignature,
	status_err: &Option<Value>,
	transaction_meta: Option<&UiTransactionStatusMeta>,
) -> Option<(u64, bool)> {
	match transaction_meta {
		Some(meta) if meta.err.is_none() != status_err.is_none() => {
			// Both are derived from the same ledger data, so this is an RPC inconsistency. Abstain
			// rather than guess.
			tracing::warn!(
				"Signature status error ({status_err:?}) and transaction meta error ({:?}) disagree for Solana transaction {signature:?}.",
				meta.err
			);
			None
		},
		Some(meta) => Some((meta.fee, status_err.is_none())),
		None => {
			// Abstaining here could stall the election indefinitely if the meta is never served,
			// so vote with the minimum fee instead.
			tracing::warn!(
				"No transaction meta for finalized Solana transaction {signature:?}, assuming the minimum fee."
			);
			Some((LAMPORTS_PER_SIGNATURE, status_err.is_none()))
		},
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{HttpEndpoint, NodeContainer},
		sol::retry_rpc::SolRetryRpcClient,
	};

	use cf_chains::{sol::SolSignature, Chain, Solana};
	use cf_utilities::task_scope;
	use futures_util::FutureExt;
	use std::str::FromStr;

	use super::*;

	const PRIORITY_FEE_TX: u64 = 15_000;

	fn meta(fee: u64, err: Option<Value>) -> UiTransactionStatusMeta {
		serde_json::from_value(serde_json::json!({
			"err": err,
			"status": { "Ok": null },
			"fee": fee,
			"preBalances": [],
			"postBalances": [],
		}))
		.unwrap()
	}

	fn instruction_error() -> Option<Value> {
		Some(serde_json::json!({ "InstructionError": [0, { "Custom": 1 }] }))
	}

	#[test]
	fn meta_present_takes_fee_from_meta() {
		assert_eq!(
			fee_and_success_status(Default::default(), &None, Some(&meta(PRIORITY_FEE_TX, None))),
			Some((PRIORITY_FEE_TX, true))
		);
		assert_eq!(
			fee_and_success_status(
				Default::default(),
				&instruction_error(),
				Some(&meta(PRIORITY_FEE_TX, instruction_error()))
			),
			Some((PRIORITY_FEE_TX, false))
		);
	}

	#[test]
	fn meta_missing_takes_success_from_status() {
		assert_eq!(
			fee_and_success_status(Default::default(), &None, None),
			Some((LAMPORTS_PER_SIGNATURE, true))
		);
		assert_eq!(
			fee_and_success_status(Default::default(), &instruction_error(), None),
			Some((LAMPORTS_PER_SIGNATURE, false))
		);
	}

	#[test]
	fn abstains_when_status_and_meta_disagree() {
		assert_eq!(
			fee_and_success_status(
				Default::default(),
				&None,
				Some(&meta(PRIORITY_FEE_TX, instruction_error()))
			),
			None
		);
		assert_eq!(
			fee_and_success_status(
				Default::default(),
				&instruction_error(),
				Some(&meta(PRIORITY_FEE_TX, None))
			),
			None
		);
	}

	#[tokio::test]
	#[ignore]
	async fn test_egress_witnessing() {
		task_scope::task_scope(|scope| {
			async {
				let client= SolRetryRpcClient::new(
						scope,
						NodeContainer {
							primary: HttpEndpoint {
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

				let (fee, tx_successful) = get_finalized_fee_and_success_status(&client, monitored_tx_signature).await.unwrap().unwrap();

				println!("{:?}", (fee, tx_successful));
				assert_eq!(fee, LAMPORTS_PER_SIGNATURE);
				assert!(tx_successful);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
