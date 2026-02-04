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

use crate::StorageQueryApi;
use bitcoin::{hashes::Hash as BtcHash, Txid};
use cf_chains::{
	address::{AddressString, EncodedAddress},
	instances::{ArbitrumInstance, BitcoinInstance, EthereumInstance},
	Chain, ChainCrypto, ChannelRefundParametersUnchecked, IntoTransactionInIdForAnyChain,
};
use cf_primitives::{BasisPoints, DcaParameters, NetworkEnvironment};
use cf_rpc_apis::RpcResult;
use cf_utilities::rpc::NumberOrHex;
use pallet_cf_broadcast::TransactionOutIdToBroadcastId;
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use serde::{Deserialize, Serialize};
use sp_api::CallApiAt;
use sp_runtime::{traits::Block as BlockT, AccountId32};
use state_chain_runtime::{
	chainflip::witnessing::ethereum_elections::{EthereumKeyManagerEvent, VaultEvents},
	Hash, Runtime,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcTransactionRef {
	Bitcoin { hash: Txid },
	Evm { hash: cf_chains::evm::H256 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcTransactionId {
	Bitcoin { hash: Txid },
	Evm { signature: cf_chains::evm::SchnorrVerificationComponents },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DepositDetails {
	Bitcoin { tx_id: Txid, vout: u32 },
	Evm { tx_hashes: Vec<cf_chains::evm::H256> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcDepositWitnessInfo {
	pub deposit_chain_block_height: u64,
	pub deposit_address: AddressString,
	pub amount: NumberOrHex,
	pub asset: cf_chains::assets::any::Asset,
	pub deposit_details: Option<DepositDetails>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BroadcastWitnessInfo {
	pub broadcast_chain_block_height: u64,
	pub broadcast_id: cf_primitives::BroadcastId,
	pub tx_out_id: RpcTransactionId,
	pub tx_ref: RpcTransactionRef,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcVaultDepositWitnessInfo {
	pub tx_id: String,
	pub deposit_chain_block_height: u64,
	pub input_asset: cf_chains::assets::any::Asset,
	pub output_asset: cf_chains::assets::any::Asset,
	pub amount: NumberOrHex,
	pub destination_address: AddressString,
	pub ccm_deposit_metadata:
		Option<cf_chains::CcmDepositMetadataUnchecked<cf_chains::ForeignChainAddress>>,
	pub deposit_details: Option<DepositDetails>,
	pub broker_fee: Option<cf_primitives::Beneficiary<AccountId32>>,
	pub affiliate_fees: Vec<cf_primitives::Beneficiary<AccountId32>>,
	pub refund_params: Option<ChannelRefundParametersUnchecked<AddressString>>,
	pub dca_params: Option<DcaParameters>,
	pub max_boost_fee: BasisPoints,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcWitnessedEventsResponse {
	pub deposits: Vec<RpcDepositWitnessInfo>,
	pub broadcasts: Vec<BroadcastWitnessInfo>,
	pub vault_deposits: Vec<RpcVaultDepositWitnessInfo>,
}

pub(crate) fn convert_deposit_witness<C: Chain>(
	witness: &DepositWitness<C>,
	height: u64,
	network: NetworkEnvironment,
) -> RpcDepositWitnessInfo
where
	C::DepositDetails: IntoRpcDepositDetails,
{
	RpcDepositWitnessInfo {
		deposit_chain_block_height: height,
		deposit_address: AddressString::from_encoded_address(
			EncodedAddress::from_chain_account::<C>(witness.deposit_address.clone(), network),
		),
		amount: <<C as cf_chains::Chain>::ChainAmount as Into<u128>>::into(witness.amount).into(),
		asset: witness.asset.into(),
		deposit_details: witness.deposit_details.clone().into_rpc_deposit_details(),
	}
}

pub(crate) fn convert_vault_deposit_witness<T, I, C, B>(
	storage_query: &StorageQueryApi<C, B>,
	hash: Hash,
	witness: &VaultDepositWitness<T, I>,
	height: u64,
	network: NetworkEnvironment,
) -> RpcResult<RpcVaultDepositWitnessInfo>
where
	T: pallet_cf_ingress_egress::Config<I, AccountId = state_chain_runtime::AccountId>,
	I: 'static,
	<T::TargetChain as Chain>::DepositDetails: IntoRpcDepositDetails,
	<T::TargetChain as Chain>::ChainAccount: Clone,
	<<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId:
		IntoTransactionInIdForAnyChain<<T::TargetChain as Chain>::ChainCrypto>,
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + CallApiAt<B>,
{
	let tx_id = <<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId::into_transaction_in_id_for_any_chain(witness.tx_id.clone())
		.to_string();

	let mut affiliate_fees = Vec::with_capacity(witness.affiliate_fees.len());
	for affiliate in &witness.affiliate_fees {
		let broker_id = witness.broker_fee.as_ref().map(|b| &b.account);
		if let Some(account) =
			resolve_affiliate_to_account(storage_query, hash, broker_id, affiliate.account)?
		{
			affiliate_fees.push(cf_primitives::Beneficiary { account, bps: affiliate.bps });
		}
	}

	let refund_params = Some(witness.refund_params.clone().map_address(|address| {
		AddressString::from_encoded_address(EncodedAddress::from_chain_account::<T::TargetChain>(
			address, network,
		))
	}));

	Ok(RpcVaultDepositWitnessInfo {
		tx_id,
		deposit_chain_block_height: height,
		input_asset: witness.input_asset.into(),
		output_asset: witness.output_asset,
		amount: <<T::TargetChain as Chain>::ChainAmount as Into<u128>>::into(
			witness.deposit_amount,
		)
		.into(),
		destination_address: AddressString::from_encoded_address(
			witness.destination_address.clone(),
		),
		ccm_deposit_metadata: witness.deposit_metadata.clone(),
		deposit_details: witness.deposit_details.clone().into_rpc_deposit_details(),
		broker_fee: witness.broker_fee.clone(),
		affiliate_fees,
		refund_params,
		dca_params: witness.dca_params.clone(),
		max_boost_fee: witness.boost_fee,
	})
}

fn resolve_affiliate_to_account<C, B>(
	storage_query: &StorageQueryApi<C, B>,
	hash: Hash,
	broker_id: Option<&state_chain_runtime::AccountId>,
	short_id: cf_primitives::AffiliateShortId,
) -> RpcResult<Option<state_chain_runtime::AccountId>>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + CallApiAt<B>,
{
	let Some(broker) = broker_id else { return Ok(None) };

	storage_query.with_state_backend(hash, || {
		pallet_cf_swapping::AffiliateIdMapping::<Runtime>::get(broker, short_id)
	})
}

fn convert_bitcoin_broadcast<C, B>(
	storage_query: &StorageQueryApi<C, B>,
	hash: Hash,
	tx_confirmation: pallet_cf_broadcast::TransactionConfirmation<Runtime, BitcoinInstance>,
	height: u64,
) -> RpcResult<Option<BroadcastWitnessInfo>>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + CallApiAt<B>,
{
	let maybe_broadcast = storage_query.with_state_backend(hash, || {
		TransactionOutIdToBroadcastId::<Runtime, BitcoinInstance>::get(tx_confirmation.tx_out_id)
	})?;
	let (broadcast_id, _) = match maybe_broadcast {
		Some(value) => value,
		None => return Ok(None),
	};

	Ok(Some(BroadcastWitnessInfo {
		broadcast_chain_block_height: height,
		broadcast_id,
		tx_out_id: RpcTransactionId::Bitcoin {
			hash: Txid::from_slice(tx_confirmation.tx_out_id.as_bytes())
				.expect("bitcoin txid hash"),
		},
		tx_ref: RpcTransactionRef::Bitcoin {
			hash: Txid::from_slice(tx_confirmation.transaction_ref.as_bytes())
				.expect("bitcoin txid hash"),
		},
	}))
}

fn convert_evm_broadcast<C, B>(
	storage_query: &StorageQueryApi<C, B>,
	hash: Hash,
	key_manager_event: &EthereumKeyManagerEvent,
	height: u64,
) -> RpcResult<Option<BroadcastWitnessInfo>>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + CallApiAt<B>,
{
	match key_manager_event {
		EthereumKeyManagerEvent::SignatureAccepted { tx_out_id, transaction_ref, .. } => {
			let maybe_broadcast = storage_query.with_state_backend(hash, || {
				TransactionOutIdToBroadcastId::<Runtime, EthereumInstance>::get(tx_out_id)
			})?;
			let (broadcast_id, _) = match maybe_broadcast {
				Some(value) => value,
				None => return Ok(None),
			};
			Ok(Some(BroadcastWitnessInfo {
				broadcast_chain_block_height: height,
				broadcast_id,
				tx_out_id: RpcTransactionId::Evm { signature: *tx_out_id },
				tx_ref: RpcTransactionRef::Evm { hash: *transaction_ref },
			}))
		},
		_ => Ok(None),
	}
}

fn extract_vault_deposit_from_event<T, I, C>(
	event: &VaultEvents<VaultDepositWitness<T, I>, C>,
) -> Option<VaultDepositWitness<T, I>>
where
	T: pallet_cf_ingress_egress::Config<I>,
	I: 'static,
	C: Chain,
	VaultDepositWitness<T, I>: Clone,
{
	match event {
		VaultEvents::SwapNativeFilter(w) |
		VaultEvents::SwapTokenFilter(w) |
		VaultEvents::XcallNativeFilter(w) |
		VaultEvents::XcallTokenFilter(w) => Some(w.clone()),
		// TransferNativeFailedFilter and TransferTokenFailedFilter don't contain vault deposits
		_ => None,
	}
}

pub(crate) trait IntoRpcDepositDetails {
	fn into_rpc_deposit_details(self) -> Option<DepositDetails>;
}

impl IntoRpcDepositDetails for cf_chains::btc::Utxo {
	fn into_rpc_deposit_details(self) -> Option<DepositDetails> {
		Some(DepositDetails::Bitcoin {
			tx_id: Txid::from_slice(self.id.tx_id.as_bytes()).expect("bitcoin txid hash"),
			vout: self.id.vout,
		})
	}
}

impl IntoRpcDepositDetails for cf_chains::evm::DepositDetails {
	fn into_rpc_deposit_details(self) -> Option<DepositDetails> {
		self.tx_hashes.map(|tx_hashes| DepositDetails::Evm { tx_hashes })
	}
}

pub(crate) fn convert_raw_witnessed_events<C, B>(
	storage_query: &StorageQueryApi<C, B>,
	hash: Hash,
	raw: state_chain_runtime::runtime_apis::custom_api::RawWitnessedEvents,
	network: NetworkEnvironment,
) -> RpcResult<RpcWitnessedEventsResponse>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + CallApiAt<B>,
{
	match raw {
		state_chain_runtime::runtime_apis::custom_api::RawWitnessedEvents::Bitcoin {
			deposits,
			broadcasts,
			vault_deposits,
		} => {
			let deposits = deposits
				.into_iter()
				.map(|(height, witness)| {
					convert_deposit_witness::<cf_chains::Bitcoin>(&witness, height, network)
				})
				.collect();

			let mut converted_vault_deposits = Vec::with_capacity(vault_deposits.len());
			for (height, witness) in vault_deposits {
				converted_vault_deposits.push(convert_vault_deposit_witness(
					storage_query,
					hash,
					&witness,
					height,
					network,
				)?);
			}

			let mut broadcasts_vec = Vec::with_capacity(broadcasts.len());
			for (height, tx) in broadcasts {
				if let Some(broadcast) = convert_bitcoin_broadcast(storage_query, hash, tx, height)?
				{
					broadcasts_vec.push(broadcast);
				}
			}

			Ok(RpcWitnessedEventsResponse {
				deposits,
				broadcasts: broadcasts_vec,
				vault_deposits: converted_vault_deposits,
			})
		},
		state_chain_runtime::runtime_apis::custom_api::RawWitnessedEvents::Ethereum {
			deposits,
			broadcasts,
			vault_deposits,
		} => {
			let deposits = deposits
				.into_iter()
				.map(|(height, witness)| {
					convert_deposit_witness::<cf_chains::Ethereum>(&witness, height, network)
				})
				.collect();

			let mut converted_vault_deposits = Vec::with_capacity(vault_deposits.len());
			for (height, event) in vault_deposits {
				if let Some(witness) = extract_vault_deposit_from_event::<
					Runtime,
					EthereumInstance,
					cf_chains::Ethereum,
				>(&event)
				{
					converted_vault_deposits.push(convert_vault_deposit_witness(
						storage_query,
						hash,
						&witness,
						height,
						network,
					)?);
				}
			}

			let mut broadcasts_vec = Vec::with_capacity(broadcasts.len());
			for (height, event) in broadcasts {
				if let Some(broadcast) = convert_evm_broadcast(storage_query, hash, &event, height)?
				{
					broadcasts_vec.push(broadcast);
				}
			}

			Ok(RpcWitnessedEventsResponse {
				deposits,
				broadcasts: broadcasts_vec,
				vault_deposits: converted_vault_deposits,
			})
		},
		state_chain_runtime::runtime_apis::custom_api::RawWitnessedEvents::Arbitrum {
			deposits,
			broadcasts,
			vault_deposits,
		} => {
			let deposits = deposits
				.into_iter()
				.map(|(height, witness)| {
					convert_deposit_witness::<cf_chains::Arbitrum>(&witness, height, network)
				})
				.collect();

			let mut converted_vault_deposits = Vec::with_capacity(vault_deposits.len());
			for (height, event) in vault_deposits {
				if let Some(witness) = extract_vault_deposit_from_event::<
					Runtime,
					ArbitrumInstance,
					cf_chains::Arbitrum,
				>(&event)
				{
					converted_vault_deposits.push(convert_vault_deposit_witness(
						storage_query,
						hash,
						&witness,
						height,
						network,
					)?);
				}
			}

			let mut broadcasts_vec = Vec::with_capacity(broadcasts.len());
			for (height, event) in broadcasts {
				if let Some(broadcast) = convert_evm_broadcast(storage_query, hash, &event, height)?
				{
					broadcasts_vec.push(broadcast);
				}
			}

			Ok(RpcWitnessedEventsResponse {
				deposits,
				broadcasts: broadcasts_vec,
				vault_deposits: converted_vault_deposits,
			})
		},
	}
}
