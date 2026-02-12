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

use bitcoin::{hashes::Hash as BtcHash, Txid};
use cf_chains::{
	address::{AddressString, EncodedAddress},
	evm::SchnorrVerificationComponents,
	instances::BitcoinInstance,
	Chain, ChainCrypto, ChannelRefundParametersUnchecked, IntoTransactionInIdForAnyChain,
};
use cf_primitives::{BasisPoints, DcaParameters, NetworkEnvironment};
use cf_traits::ChainflipWithTargetChain;
use cf_utilities::rpc::NumberOrHex;
use pallet_cf_broadcast::{TransactionConfirmation, TransactionOutIdToBroadcastId};
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use serde::{Deserialize, Serialize};
use sp_core::H256;
use sp_runtime::AccountId32;
use state_chain_runtime::{
	chainflip::witnessing::pallet_hooks::{
		Config as ConfigTrait, EvmKeyManagerEvent, EvmVaultContractEvent,
	},
	Runtime,
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

pub(crate) fn convert_vault_deposit_witness<T, I>(
	witness: &VaultDepositWitness<T, I>,
	height: u64,
	network: NetworkEnvironment,
) -> RpcVaultDepositWitnessInfo
where
	T: pallet_cf_ingress_egress::Config<I, AccountId = state_chain_runtime::AccountId>,
	I: 'static,
	<T::TargetChain as Chain>::DepositDetails: IntoRpcDepositDetails,
	<T::TargetChain as Chain>::ChainAccount: Clone,
	<<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId:
		IntoTransactionInIdForAnyChain<<T::TargetChain as Chain>::ChainCrypto>,
{
	let tx_id = <<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId::into_transaction_in_id_for_any_chain(witness.tx_id.clone())
		.to_string();

	let mut affiliate_fees = Vec::with_capacity(witness.affiliate_fees.len());
	for affiliate in &witness.affiliate_fees {
		let broker_id = witness.broker_fee.as_ref().map(|b| &b.account);
		if let Some(account) = resolve_affiliate_to_account(broker_id, affiliate.account) {
			affiliate_fees.push(cf_primitives::Beneficiary { account, bps: affiliate.bps });
		}
	}

	let refund_params = Some(witness.refund_params.clone().map_address(|address| {
		AddressString::from_encoded_address(EncodedAddress::from_chain_account::<T::TargetChain>(
			address, network,
		))
	}));

	RpcVaultDepositWitnessInfo {
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
	}
}

fn resolve_affiliate_to_account(
	broker_id: Option<&state_chain_runtime::AccountId>,
	short_id: cf_primitives::AffiliateShortId,
) -> Option<state_chain_runtime::AccountId> {
	let broker = broker_id?;
	pallet_cf_swapping::AffiliateIdMapping::<Runtime>::get(broker, short_id)
}

fn convert_bitcoin_broadcast(
	tx_confirmation: pallet_cf_broadcast::TransactionConfirmation<Runtime, BitcoinInstance>,
	height: u64,
) -> Option<BroadcastWitnessInfo> {
	let (broadcast_id, _) =
		TransactionOutIdToBroadcastId::<Runtime, BitcoinInstance>::get(tx_confirmation.tx_out_id)?;

	Some(BroadcastWitnessInfo {
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
	})
}

fn convert_evm_broadcast<I: 'static>(
	key_manager_event: &EvmKeyManagerEvent<Runtime, I>,
	height: u64,
) -> Option<BroadcastWitnessInfo>
where
	Runtime: ConfigTrait<I>,
	<<Runtime as ChainflipWithTargetChain<I>>::TargetChain as Chain>::ChainCrypto:
		ChainCrypto<TransactionOutId = SchnorrVerificationComponents>,
	<Runtime as ChainflipWithTargetChain<I>>::TargetChain: Chain<TransactionRef = H256>,
{
	match key_manager_event {
		EvmKeyManagerEvent::SignatureAccepted(TransactionConfirmation {
			tx_out_id,
			transaction_ref,
			..
		}) => {
			let (broadcast_id, _) = TransactionOutIdToBroadcastId::<Runtime, I>::get(tx_out_id)?;
			Some(BroadcastWitnessInfo {
				broadcast_chain_block_height: height,
				broadcast_id,
				tx_out_id: RpcTransactionId::Evm { signature: *tx_out_id },
				tx_ref: RpcTransactionRef::Evm { hash: *transaction_ref },
			})
		},
		_ => None,
	}
}

fn extract_vault_deposit_from_event<T, I>(
	event: &EvmVaultContractEvent<T, I>,
) -> Option<VaultDepositWitness<T, I>>
where
	T: pallet_cf_ingress_egress::Config<I>,
	I: 'static,
	VaultDepositWitness<T, I>: Clone,
{
	match event {
		EvmVaultContractEvent::VaultDeposit(w) => Some(*w.clone()),
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

pub(crate) fn convert_raw_witnessed_events(
	raw: state_chain_runtime::runtime_apis::custom_api::RawWitnessedEvents,
	network: NetworkEnvironment,
) -> RpcWitnessedEventsResponse {
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

			let converted_vault_deposits = vault_deposits
				.into_iter()
				.map(|(height, witness)| convert_vault_deposit_witness(&witness, height, network))
				.collect();

			let broadcasts_vec = broadcasts
				.into_iter()
				.filter_map(|(height, tx)| convert_bitcoin_broadcast(tx, height))
				.collect();

			RpcWitnessedEventsResponse {
				deposits,
				broadcasts: broadcasts_vec,
				vault_deposits: converted_vault_deposits,
			}
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

			let converted_vault_deposits = vault_deposits
				.into_iter()
				.filter_map(|(height, event)| {
					extract_vault_deposit_from_event(&event)
						.map(|witness| convert_vault_deposit_witness(&witness, height, network))
				})
				.collect();

			let broadcasts_vec = broadcasts
				.into_iter()
				.filter_map(|(height, event)| convert_evm_broadcast(&event, height))
				.collect();

			RpcWitnessedEventsResponse {
				deposits,
				broadcasts: broadcasts_vec,
				vault_deposits: converted_vault_deposits,
			}
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

			let converted_vault_deposits = vault_deposits
				.into_iter()
				.filter_map(|(height, event)| {
					extract_vault_deposit_from_event(&event)
						.map(|witness| convert_vault_deposit_witness(&witness, height, network))
				})
				.collect();

			let broadcasts_vec = broadcasts
				.into_iter()
				.filter_map(|(height, event)| convert_evm_broadcast(&event, height))
				.collect();

			RpcWitnessedEventsResponse {
				deposits,
				broadcasts: broadcasts_vec,
				vault_deposits: converted_vault_deposits,
			}
		},
	}
}
