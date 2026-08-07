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

pub mod hub_deposits;

use cf_chains::dot::{
	PolkadotAccountId, PolkadotBalance, PolkadotExtrinsicIndex, PolkadotSignature,
	PolkadotTransactionId,
};
use pallet_cf_broadcast::TransactionConfirmation;
use state_chain_runtime::{AssethubInstance, Runtime};
use subxt::{
	backend::legacy::rpc_methods::Bytes,
	config::PolkadotConfig,
	events::{EventDetails, Phase, StaticEvent},
	utils::{AccountId32, MultiAddress, MultiSignature},
};

use tracing::error;

use std::collections::BTreeSet;

use crate::{
	dot::cached_rpc::DotRetryRpcApiWithResult, witness::hub_elections::AssethubBlockHeader,
};

// To generate the metadata file, use the subxt-cli tool (`cargo install subxt-cli`):
// subxt metadata --version=14 --pallets Proxy,Balances,TransactionPayment,System,Assets --url
// wss://polkadot-asset-hub-rpc.polkadot.io:443 > metadata.assethub.scale
//
// Make sure to use subxt version 0.41 to generate the metadata file.
#[subxt::subxt(runtime_metadata_path = "metadata.assethub.scale")]
pub mod assethub {}

pub type HubAssetId = u32;

#[derive(Debug, Clone)]
pub enum EventWrapper {
	ProxyAdded {
		delegator: AccountId32,
		delegatee: AccountId32,
	},
	BalancesTransfer {
		to: AccountId32,
		from: AccountId32,
		amount: PolkadotBalance,
	},
	AssetsTransfer {
		asset_id: HubAssetId,
		to: AccountId32,
		from: AccountId32,
		amount: PolkadotBalance,
	},
	TransactionFeePaid {
		actual_fee: PolkadotBalance,
		tip: PolkadotBalance,
	},
	ExtrinsicSuccess,
}

use assethub::{
	assets::events::Transferred as AssetsTransferred, balances::events::Transfer,
	proxy::events::ProxyAdded, system::events::ExtrinsicSuccess,
	transaction_payment::events::TransactionFeePaid,
};

pub fn filter_map_events(
	res_event_details: Result<EventDetails<PolkadotConfig>, subxt::Error>,
) -> Option<(Phase, EventWrapper)> {
	match res_event_details {
		Ok(event_details) => match (event_details.pallet_name(), event_details.variant_name()) {
			(ProxyAdded::PALLET, ProxyAdded::EVENT) => {
				let ProxyAdded { delegator, delegatee, .. } =
					event_details.as_event::<ProxyAdded>().unwrap().unwrap();
				Some(EventWrapper::ProxyAdded { delegator, delegatee })
			},
			(Transfer::PALLET, Transfer::EVENT) => {
				let Transfer { to, amount, from } =
					event_details.as_event::<Transfer>().unwrap().unwrap();
				Some(EventWrapper::BalancesTransfer { to, amount, from })
			},
			(TransactionFeePaid::PALLET, TransactionFeePaid::EVENT) => {
				let TransactionFeePaid { actual_fee, tip, .. } =
					event_details.as_event::<TransactionFeePaid>().unwrap().unwrap();
				Some(EventWrapper::TransactionFeePaid { actual_fee, tip })
			},
			(ExtrinsicSuccess::PALLET, ExtrinsicSuccess::EVENT) => {
				let ExtrinsicSuccess { .. } =
					event_details.as_event::<ExtrinsicSuccess>().unwrap().unwrap();
				Some(EventWrapper::ExtrinsicSuccess)
			},
			(AssetsTransferred::PALLET, AssetsTransferred::EVENT) => {
				let AssetsTransferred { asset_id, from, to, amount } =
					event_details.as_event::<AssetsTransferred>().unwrap().unwrap();
				Some(EventWrapper::AssetsTransfer { asset_id, to, amount, from })
			},
			_ => None,
		}
		.map(|event| (event_details.phase(), event)),
		Err(err) => {
			error!("Error while parsing event: {:?}", err);
			None
		},
	}
}

fn extract_state_chain_signer_and_signature(
	raw_extrinsic: &[u8],
) -> Option<(PolkadotAccountId, PolkadotSignature)> {
	const LEGACY_EXTRINSIC_FORMAT_VERSION: u8 = 4;
	const VERSION_MASK: u8 = 0b0011_1111;
	const TYPE_MASK: u8 = 0b1100_0000;
	const SIGNED_EXTRINSIC: u8 = 0b1000_0000;

	use codec::{Decode, Input};

	let mut input = raw_extrinsic;

	let _length = <codec::Compact<u32>>::decode(&mut input).ok()?;
	let version_and_type = input.read_byte().ok()?;

	let version = version_and_type & VERSION_MASK;
	let xt_type = version_and_type & TYPE_MASK;

	match (version, xt_type) {
		(LEGACY_EXTRINSIC_FORMAT_VERSION, SIGNED_EXTRINSIC) => {
			let signer = match MultiAddress::<AccountId32, u32>::decode(&mut input).ok()? {
				MultiAddress::Id(account_id) => PolkadotAccountId(account_id.0),
				MultiAddress::Address32(account_id_bytes) => PolkadotAccountId(account_id_bytes),
				MultiAddress::Index(_) | MultiAddress::Raw(_) | MultiAddress::Address20(_) =>
					return None,
			};
			let signature = MultiSignature::decode(&mut input).ok()?;

			// we only use the Sr25519 for threshold signatures, so we only look out for them
			match signature {
				MultiSignature::Ed25519(_) => None,
				MultiSignature::Sr25519(polkadot_signature) =>
					Some((signer, PolkadotSignature::from_aliased(polkadot_signature))),
				MultiSignature::Ecdsa(_) => None,
			}
		},
		_ => None,
	}
}

pub async fn process_egresses_in_block(
	hub_client: &impl DotRetryRpcApiWithResult,
	pending_tx_signatures: &[PolkadotSignature],
	header: &AssethubBlockHeader,
) -> anyhow::Result<Vec<TransactionConfirmation<Runtime, AssethubInstance>>> {
	let mut transaction_confirmations = Vec::new();

	let monitored_egress_ids: BTreeSet<_> = pending_tx_signatures.iter().cloned().collect();

	// all indices of all successful extrinsics. This includes both egresses and
	// extrinsics that caused ProxyAdded events.
	let extrinsic_indices = extrinsic_success_indices(&header.events);

	let extrinsics: Vec<Bytes> = hub_client.extrinsics(header.block_hash).await?;

	for (extrinsic_index, tx_fee) in transaction_fee_paids(&extrinsic_indices, &header.events) {
		let xt = extrinsics.get(extrinsic_index as usize).expect(
			"We know this exists since we got
	this index from the event, from the block we are querying.",
		);

		match extract_state_chain_signer_and_signature(&xt.0[..]) {
			Some((signer, signature)) =>
				if monitored_egress_ids.contains(&signature) {
					tracing::info!(
						"Witnessing Assethub transaction succeeded. signature: {signature:?}"
					);
					transaction_confirmations.push(pallet_cf_broadcast::TransactionConfirmation {
						tx_out_id: signature,
						signer_id: signer, /* this is the account that gets refunded for
						                    * submitting this tx */
						tx_fee,
						tx_metadata: (),
						transaction_ref: PolkadotTransactionId {
							block_number: header.block_height,
							extrinsic_index,
						},
					});
				},
			None => {
				// We expect this to occur when attempting to decode v5 or bare extrinsics.
				tracing::debug!(
					"Unable to extract signature for extrinsic {}:{}.",
					header.block_hash,
					extrinsic_index,
				);
			},
		}
	}

	Ok(transaction_confirmations)
}

fn transaction_fee_paids(
	indices: &BTreeSet<PolkadotExtrinsicIndex>,
	events: &[(Phase, EventWrapper)],
) -> BTreeSet<(PolkadotExtrinsicIndex, PolkadotBalance)> {
	events
		.iter()
		.filter_map(|(phase, wrapped_event)| match (phase, wrapped_event) {
			(
				Phase::ApplyExtrinsic(extrinsic_index),
				EventWrapper::TransactionFeePaid { actual_fee, .. },
			) if indices.contains(extrinsic_index) => Some((*extrinsic_index, *actual_fee)),
			_ => None,
		})
		.collect()
}

fn extrinsic_success_indices(events: &[(Phase, EventWrapper)]) -> BTreeSet<PolkadotExtrinsicIndex> {
	events
		.iter()
		.filter_map(|(phase, wrapped_event)| match (phase, wrapped_event) {
			(Phase::ApplyExtrinsic(extrinsic_index), EventWrapper::ExtrinsicSuccess) =>
				Some(*extrinsic_index),
			_ => None,
		})
		.collect()
}

#[cfg(test)]
pub mod test {
	use super::*;

	pub fn phase_and_events(
		events: Vec<(PolkadotExtrinsicIndex, EventWrapper)>,
	) -> Vec<(Phase, EventWrapper)> {
		events
			.into_iter()
			.map(|(xt_index, event)| (Phase::ApplyExtrinsic(xt_index), event))
			.collect()
	}

	fn mock_tx_fee_paid(actual_fee: PolkadotBalance) -> EventWrapper {
		EventWrapper::TransactionFeePaid { actual_fee, tip: Default::default() }
	}

	#[tokio::test]
	async fn test_extrinsic_success_filtering() {
		let events = phase_and_events(vec![
			(1u32, EventWrapper::ExtrinsicSuccess),
			(2u32, mock_tx_fee_paid(20000)),
			(2u32, EventWrapper::ExtrinsicSuccess),
			(3u32, mock_tx_fee_paid(20000)),
		]);

		assert_eq!(extrinsic_success_indices(&events), BTreeSet::from([1, 2]));
	}
}
