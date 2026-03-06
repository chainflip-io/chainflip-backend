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

use crate::evm::event::Event;
use anyhow::{anyhow, Result};
use codec::Decode;
use itertools::Itertools;
use sp_core::Get;
use std::{collections::HashMap, fmt::Debug};

use cf_chains::{
	address::EncodedAddress,
	cf_parameters::VaultSwapParametersV1,
	evm::{DepositDetails, EvmChain, H256},
	CcmChannelMetadata, CcmDepositMetadata, Chain,
};
use cf_primitives::{AssetAmount, ForeignChain};
use ethers::prelude::*;
use pallet_cf_ingress_egress::{TransferFailedWitness, VaultDepositWitness};
use state_chain_runtime::chainflip::witnessing::pallet_hooks::EvmVaultContractEvent;

abigen!(Vault, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IVault.json");

pub fn decode_cf_parameters<RefundAddress, CcmData>(
	cf_parameters: &[u8],
	block_query: impl Debug,
) -> Result<(VaultSwapParametersV1<RefundAddress>, CcmData)>
where
	RefundAddress: Decode,
	CcmData: Default + Decode,
{
	cf_chains::cf_parameters::decode_cf_parameters(cf_parameters)
		.inspect_err(|_| {
			tracing::warn!(
				"Failed to decode cf_parameters: {cf_parameters:?} at block {block_query:?}"
			)
		})
		.map_err(|e| anyhow!(e))
}

macro_rules! vault_deposit_witness {
	($source_asset: expr, $deposit_amount: expr, $dest_asset: expr, $dest_address: expr, $metadata: expr, $tx_id: expr, $params: expr) => {
		VaultDepositWitness {
			input_asset: $source_asset,
			output_asset: $dest_asset,
			deposit_amount: $deposit_amount,
			destination_address: $dest_address,
			deposit_metadata: $metadata,
			tx_id: $tx_id,
			deposit_details: DepositDetails { tx_hashes: Some(vec![$tx_id]) },
			broker_fee: Some($params.broker_fee),
			affiliate_fees: $params.affiliate_fees
				.into_iter()
				.map(Into::into)
				.collect_vec()
				.try_into()
				.expect("runtime supports at least as many affiliates as we allow in cf_parameters encoding"),
			boost_fee: $params.boost_fee.into(),
			dca_params: $params.dca_params,
			refund_params: $params.refund_params,
			channel_id: None,
			deposit_address: None,
		}
	}
}

fn try_into_primitive<Primitive: std::fmt::Debug + TryInto<CfType> + Copy, CfType>(
	from: Primitive,
) -> Result<CfType>
where
	<Primitive as TryInto<CfType>>::Error: std::fmt::Display,
{
	from.try_into().map_err(|err| {
		anyhow!("Failed to convert into {:?}: {err}", std::any::type_name::<CfType>(),)
	})
}

fn try_into_encoded_address(chain: ForeignChain, bytes: Vec<u8>) -> Result<EncodedAddress> {
	EncodedAddress::from_chain_bytes(chain, bytes)
		.map_err(|e| anyhow!("Failed to convert into EncodedAddress: {e}"))
}

pub fn handle_vault_events<
	T: pallet_cf_ingress_egress::Config<
		I,
		TargetChain: EvmChain,
		AccountId = cf_primitives::AccountId,
	>,
	I: 'static,
>(
	supported_assets: &HashMap<H160, <T::TargetChain as Chain>::ChainAsset>,
	events: Vec<Event<VaultEvents>>,
	block_query: &impl Debug,
) -> Result<Vec<EvmVaultContractEvent<T, I>>> {
	let mut result = Vec::new();

	for event in events {
		if let Some(mapped) = handle_vault_event::<T, I>(
			supported_assets,
			event.event_parameters,
			event.tx_hash,
			block_query,
		)? {
			result.push(mapped);
		}
	}

	Ok(result)
}

fn handle_vault_event<
	T: pallet_cf_ingress_egress::Config<
		I,
		TargetChain: EvmChain,
		AccountId = cf_primitives::AccountId,
	>,
	I: 'static,
>(
	supported_assets: &HashMap<H160, <T::TargetChain as Chain>::ChainAsset>,
	event: VaultEvents,
	tx_hash: H256,
	block_query: &impl Debug,
) -> Result<Option<EvmVaultContractEvent<T, I>>> {
	Ok(Some(match event {
		VaultEvents::SwapNativeFilter(SwapNativeFilter {
			dst_chain,
			dst_address,
			dst_token,
			amount,
			sender: _,
			cf_parameters,
		}) => {
			let (vault_swap_parameters, ()) =
				decode_cf_parameters(&cf_parameters[..], block_query)?;

			EvmVaultContractEvent::VaultDeposit(Box::new(vault_deposit_witness!(
				<T::TargetChain as Chain>::GAS_ASSET,
				try_into_primitive(amount).map_err(|e| anyhow!("Failed to convert amount: {e}"))?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				None,
				tx_hash,
				vault_swap_parameters
			)))
		},
		VaultEvents::SwapTokenFilter(SwapTokenFilter {
			dst_chain,
			dst_address,
			dst_token,
			src_token,
			amount,
			sender: _,
			cf_parameters,
		}) => {
			let (vault_swap_parameters, ()) =
				decode_cf_parameters(&cf_parameters[..], block_query)?;

			let asset = supported_assets
				.get(&src_token)
				.ok_or_else(|| anyhow!("Source token {src_token:?} not found"))?;

			EvmVaultContractEvent::VaultDeposit(Box::new(vault_deposit_witness!(
				*asset,
				try_into_primitive(amount).map_err(|e| anyhow!("Failed to convert amount: {e}"))?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				None,
				tx_hash,
				vault_swap_parameters
			)))
		},
		VaultEvents::XcallNativeFilter(XcallNativeFilter {
			dst_chain,
			dst_address,
			dst_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		}) => {
			let (vault_swap_parameters, ccm_additional_data) =
				decode_cf_parameters(&cf_parameters[..], block_query)?;

			EvmVaultContractEvent::VaultDeposit(Box::new(vault_deposit_witness!(
				<T::TargetChain as Chain>::GAS_ASSET,
				try_into_primitive(amount).map_err(|e| anyhow!("Failed to convert amount: {e}"))?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				Some(CcmDepositMetadata {
					source_chain: <T::TargetChain as Get<ForeignChain>>::get(),
					source_address: Some(T::TargetChain::chain_account_to_foreign_chain_address(
						sender
					)),
					channel_metadata: CcmChannelMetadata {
						message: message
							.to_vec()
							.try_into()
							.map_err(|_| anyhow!("Failed to deposit CCM: `message` too long."))?,
						gas_budget: try_into_primitive(gas_amount)?,
						ccm_additional_data,
					},
				}),
				tx_hash,
				vault_swap_parameters
			)))
		},
		VaultEvents::XcallTokenFilter(XcallTokenFilter {
			dst_chain,
			dst_address,
			dst_token,
			src_token,
			amount,
			sender,
			message,
			gas_amount,
			cf_parameters,
		}) => {
			let (vault_swap_parameters, ccm_additional_data) =
				decode_cf_parameters(&cf_parameters[..], block_query)?;

			let asset = supported_assets
				.get(&src_token)
				.ok_or_else(|| anyhow!("Source token {src_token:?} not found"))?;

			EvmVaultContractEvent::VaultDeposit(Box::new(vault_deposit_witness!(
				*asset,
				try_into_primitive(amount).map_err(|e| anyhow!("Failed to convert amount: {e}"))?,
				try_into_primitive(dst_token)?,
				try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?,
				Some(CcmDepositMetadata {
					source_chain: <T::TargetChain as Get<ForeignChain>>::get(),
					source_address: Some(T::TargetChain::chain_account_to_foreign_chain_address(
						sender
					)),
					channel_metadata: CcmChannelMetadata {
						message: message
							.to_vec()
							.try_into()
							.map_err(|_| anyhow!("Failed to deposit CCM. Message too long."))?,
						gas_budget: try_into_primitive(gas_amount)?,
						ccm_additional_data,
					},
				}),
				tx_hash,
				vault_swap_parameters
			)))
		},
		VaultEvents::TransferNativeFailedFilter(TransferNativeFailedFilter {
			recipient,
			amount,
		}) => EvmVaultContractEvent::TransferFailed(TransferFailedWitness {
			asset: <T::TargetChain as Chain>::GAS_ASSET,
			amount: try_into_primitive::<_, AssetAmount>(amount)?,
			destination_address: recipient,
		}),
		VaultEvents::TransferTokenFailedFilter(TransferTokenFailedFilter {
			recipient,
			amount,
			token,
			reason: _,
		}) => {
			let asset = supported_assets
				.get(&token)
				.ok_or_else(|| anyhow!("Asset {token:?} not found"))?;

			EvmVaultContractEvent::TransferFailed(TransferFailedWitness {
				asset: *asset,
				amount: try_into_primitive(amount)?,
				destination_address: recipient,
			})
		},
		_ => return Ok(None),
	}))
}
