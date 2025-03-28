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

use crate::*;

use cf_chains::sol::{api::SolanaApi, SolAddress};
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};
use pallet_cf_broadcast::{TransactionFor, TransactionOutIdFor};
use scale_info::TypeInfo;

pub mod old {
	use super::*;
	use cf_chains::sol::api::SolanaTransactionType;
	use cf_primitives::{AuthorityCount, ThresholdSignatureRequestId};
	use pallet_cf_threshold_signature::ThresholdCeremonyType;
	use sol_prim::transaction::legacy::DeprecatedLegacyMessage;
	use sp_runtime::AccountId32;

	#[derive(Encode, Decode)]
	pub struct RequestContext {
		pub request_id: u32,
		/// The number of ceremonies attempted so far, excluding the current one.
		/// Currently we do not limit the number of retry attempts for ceremony type Standard.
		/// Most transactions are critical, so we should retry until success.
		pub attempt_count: AuthorityCount,
		/// The payload to be signed over.
		pub payload: DeprecatedLegacyMessage,
	}

	#[derive(Encode, Decode)]
	pub struct CeremonyContext {
		pub request_context: RequestContext,
		/// The respondents that have yet to reply.
		pub remaining_respondents: BTreeSet<AccountId32>,
		/// The number of blame votes (accusations) each authority has received.
		pub blame_counts: BTreeMap<AccountId32, AuthorityCount>,
		/// The candidates participating in the signing ceremony (ie. the threshold set).
		pub candidates: BTreeSet<AccountId32>,
		/// The epoch in which the ceremony was started.
		pub epoch: EpochIndex,
		/// The key we want to sign with.
		pub key: SolAddress,
		/// Determines how/if we deal with ceremony failure.
		pub threshold_ceremony_type: ThresholdCeremonyType,
	}

	#[derive(Encode, Decode, TypeInfo)]
	pub enum BroadcastCall {
		#[codec(index = 1)]
		OnSignatureReady {
			threshold_request_id: ThresholdSignatureRequestId,
			threshold_signature_payload: DeprecatedLegacyMessage,
			api_call: SolanaApi,
			broadcast_id: BroadcastId,
			initiated_at: u64,
			should_broadcast: bool,
		},
	}

	#[derive(Encode, Decode, TypeInfo)]
	pub enum RuntimeCall {
		#[codec(index = 43)]
		Broadcast(BroadcastCall),
	}

	#[derive(Encode, Decode, TypeInfo)]
	pub struct BroadcastData<T: pallet_cf_broadcast::Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub transaction_payload: TransactionFor<T, I>,
		pub threshold_signature_payload: DeprecatedLegacyMessage,
		pub transaction_out_id: TransactionOutIdFor<T, I>,
		pub nominee: Option<T::ValidatorId>,
	}

	#[derive(Encode, Decode, TypeInfo)]
	pub struct LegacyTransaction {
		pub signatures: Vec<sol_prim::Signature>,
		pub message: DeprecatedLegacyMessage,
	}

	#[derive(Encode, Decode, TypeInfo)]
	pub struct SolanaApi {
		pub call_type: SolanaTransactionType,
		pub transaction: LegacyTransaction,
		pub signer: Option<SolAddress>,
	}
}

pub struct SolVersionedTransactionBroadcastPallet;

impl UncheckedOnRuntimeUpgrade for SolVersionedTransactionBroadcastPallet {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌞 Running Broadcast Pallet migrations for Solana versioned transactions.");

		pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::translate_values::<
			old::BroadcastData<Runtime, SolanaInstance>,
			_,
		>(|old_value| {
			Some(pallet_cf_broadcast::BroadcastData {
				broadcast_id: old_value.broadcast_id,
				transaction_payload: old_value.transaction_payload,
				threshold_signature_payload: sol_prim::transaction::VersionedMessage::Legacy(
					old_value.threshold_signature_payload,
				),
				transaction_out_id: old_value.transaction_out_id,
				nominee: old_value.nominee,
			})
		});
		pallet_cf_broadcast::PendingApiCalls::<Runtime, SolanaInstance>::translate_values::<
			old::SolanaApi,
			_,
		>(|old_value| {
			Some(SolanaApi {
				call_type: old_value.call_type,
				transaction: sol_prim::transaction::VersionedTransaction {
					signatures: old_value.transaction.signatures,
					message: sol_prim::transaction::VersionedMessage::Legacy(
						old_value.transaction.message,
					),
				},
				signer: old_value.signer,
				_phantom: Default::default(),
			})
		});

		Weight::zero()
	}
}

pub struct SolVersionedTransactionThresholdSignerPallet;

impl UncheckedOnRuntimeUpgrade for SolVersionedTransactionThresholdSignerPallet {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌞 Running Threshold Pallet migrations for Solana versioned transactions.");

		pallet_cf_threshold_signature::RequestCallback::<Runtime, SolanaInstance>::translate_values::<
			old::RuntimeCall,
			_,
		>(|old_value| match old_value {
			old::RuntimeCall::Broadcast(old::BroadcastCall::OnSignatureReady {
				threshold_request_id,
				threshold_signature_payload,
				api_call,
				broadcast_id,
				initiated_at,
				should_broadcast,
			}) => Some(crate::RuntimeCall::SolanaBroadcaster(
				pallet_cf_broadcast::Call::on_signature_ready {
					threshold_request_id,
					threshold_signature_payload: sol_prim::transaction::VersionedMessage::Legacy(
						threshold_signature_payload,
					),
					api_call: Box::new(SolanaApi {
						call_type: api_call.call_type,
						transaction: sol_prim::transaction::VersionedTransaction {
							signatures: api_call.transaction.signatures,
							message: sol_prim::transaction::VersionedMessage::Legacy(
								api_call.transaction.message,
							),
						},
						signer: api_call.signer,
						_phantom: Default::default(),
					}),
					broadcast_id,
					initiated_at,
					should_broadcast,
				},
			)),
		});
		pallet_cf_threshold_signature::PendingCeremonies::<Runtime, SolanaInstance>::translate_values::<
			old::CeremonyContext,
			_,
		>(|old_value| {
			Some(pallet_cf_threshold_signature::CeremonyContext {
				request_context: pallet_cf_threshold_signature::RequestContext {
					request_id: old_value.request_context.request_id,
					attempt_count: old_value.request_context.attempt_count,
					payload: sol_prim::transaction::VersionedMessage::Legacy(old_value.request_context.payload),
				},
				remaining_respondents: old_value.remaining_respondents,
				blame_counts: old_value.blame_counts,
				candidates: old_value.candidates,
				epoch: old_value.epoch,
				key: old_value.key,
				threshold_ceremony_type: old_value.threshold_ceremony_type,
			})
		});

		Weight::zero()
	}
}
