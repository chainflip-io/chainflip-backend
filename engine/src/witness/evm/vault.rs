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
use pallet_cf_ingress_egress::{
	TransferFailedWitness, VaultDepositFacts, VaultDepositWitness, VaultSwapData,
};
use state_chain_runtime::chainflip::witnessing::pallet_hooks::EvmVaultContractEvent;

abigen!(Vault, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IVault.json");

pub fn decode_cf_parameters<RefundAddress, CcmData>(
	cf_parameters: &[u8],
) -> Result<(VaultSwapParametersV1<RefundAddress>, CcmData)>
where
	RefundAddress: Decode,
	CcmData: Default + Decode,
{
	cf_chains::cf_parameters::decode_cf_parameters(cf_parameters).map_err(|e| {
		anyhow!("Failed to decode cf_parameters 0x{}: {e}", hex::encode(cf_parameters))
	})
}

macro_rules! vault_swap_data {
	($dest_asset: expr, $dest_address: expr, $metadata: expr, $params: expr) => {
		pallet_cf_ingress_egress::VaultSwapData {
			output_asset: $dest_asset,
			destination_address: $dest_address,
			deposit_metadata: $metadata,
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
		}
	}
}

pub(crate) use vault_swap_data;

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

	// Identical vault deposits in one transaction intentionally share a witness and collapse
	// during election deduplication. Deposits with different witnessed fields remain distinct.
	for event in events {
		match handle_vault_event::<T, I>(
			supported_assets,
			event.event_parameters,
			event.tx_hash,
			block_query,
		) {
			Ok(Some(mapped)) => result.push(mapped),
			Ok(None) => {},
			Err(e) => {
				tracing::warn!(tx_hash = ?event.tx_hash, "Ignoring vault contract event at {block_query:?}: {e:#}");
			},
		}
	}

	Ok(result)
}

/// The user-supplied part of a vault swap event, which may be malformed.
struct SwapRequest {
	dst_chain: u32,
	dst_address: Bytes,
	dst_token: u32,
	cf_parameters: Bytes,
	/// Sender, message and gas amount of a cross-chain message.
	ccm: Option<(H160, Bytes, U256)>,
}

fn decode_vault_swap<
	T: pallet_cf_ingress_egress::Config<
		I,
		TargetChain: EvmChain,
		AccountId = cf_primitives::AccountId,
	>,
	I: 'static,
>(
	SwapRequest { dst_chain, dst_address, dst_token, cf_parameters, ccm }: SwapRequest,
) -> Result<VaultSwapData<T, I>> {
	let output_asset = try_into_primitive(dst_token)?;
	let destination_address =
		try_into_encoded_address(try_into_primitive(dst_chain)?, dst_address.to_vec())?;
	Ok(match ccm {
		None => {
			let (vault_swap_parameters, ()) = decode_cf_parameters(&cf_parameters[..])?;
			vault_swap_data!(output_asset, destination_address, None, vault_swap_parameters)
		},
		Some((sender, message, gas_amount)) => {
			let (vault_swap_parameters, ccm_additional_data) =
				decode_cf_parameters(&cf_parameters[..])?;
			vault_swap_data!(
				output_asset,
				destination_address,
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
				vault_swap_parameters
			)
		},
	})
}

/// A malformed `swap` produces a `Failed` witness, since the deposit itself is verified.
/// Errors in the deposit facts (unknown token, amount overflow) mean the event is ignored.
fn vault_deposit<
	T: pallet_cf_ingress_egress::Config<
		I,
		TargetChain: EvmChain,
		AccountId = cf_primitives::AccountId,
	>,
	I: 'static,
>(
	input_asset: <T::TargetChain as Chain>::ChainAsset,
	amount: U256,
	tx_hash: H256,
	block_query: &impl Debug,
	swap: SwapRequest,
) -> Result<EvmVaultContractEvent<T, I>> {
	let deposit = VaultDepositFacts {
		input_asset,
		deposit_amount: try_into_primitive(amount)
			.map_err(|e| anyhow!("Failed to convert amount: {e}"))?,
		tx_id: tx_hash,
		deposit_details: DepositDetails { tx_hashes: Some(vec![tx_hash]) },
		channel_id: None,
		deposit_address: None,
	};
	let witness = match decode_vault_swap::<T, I>(swap) {
		Ok(swap) => VaultDepositWitness::Swap { deposit, swap },
		Err(e) => {
			tracing::warn!(?tx_hash, "Failed to decode vault swap at {block_query:?}: {e:#}");
			VaultDepositWitness::Failed { deposit, broker_id: None }
		},
	};
	Ok(EvmVaultContractEvent::VaultDeposit(Box::new(witness)))
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
		}) => vault_deposit::<T, I>(
			<T::TargetChain as Chain>::GAS_ASSET,
			amount,
			tx_hash,
			block_query,
			SwapRequest { dst_chain, dst_address, dst_token, cf_parameters, ccm: None },
		)?,
		VaultEvents::SwapTokenFilter(SwapTokenFilter {
			dst_chain,
			dst_address,
			dst_token,
			src_token,
			amount,
			sender: _,
			cf_parameters,
		}) => {
			let asset = supported_assets
				.get(&src_token)
				.ok_or_else(|| anyhow!("Source token {src_token:?} not found"))?;

			vault_deposit::<T, I>(
				*asset,
				amount,
				tx_hash,
				block_query,
				SwapRequest { dst_chain, dst_address, dst_token, cf_parameters, ccm: None },
			)?
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
		}) => vault_deposit::<T, I>(
			<T::TargetChain as Chain>::GAS_ASSET,
			amount,
			tx_hash,
			block_query,
			SwapRequest {
				dst_chain,
				dst_address,
				dst_token,
				cf_parameters,
				ccm: Some((sender, message, gas_amount)),
			},
		)?,
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
			let asset = supported_assets
				.get(&src_token)
				.ok_or_else(|| anyhow!("Source token {src_token:?} not found"))?;

			vault_deposit::<T, I>(
				*asset,
				amount,
				tx_hash,
				block_query,
				SwapRequest {
					dst_chain,
					dst_address,
					dst_token,
					cf_parameters,
					ccm: Some((sender, message, gas_amount)),
				},
			)?
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

#[cfg(test)]
mod tests {
	use state_chain_runtime::{EthereumInstance, Runtime};

	use super::*;

	fn make_event(event_parameters: VaultEvents) -> Event<VaultEvents> {
		Event { tx_hash: H256::default(), log_index: U256::zero(), event_parameters }
	}

	#[test]
	fn malformed_swap_is_witnessed_alongside_other_events() {
		let supported_assets: HashMap<H160, <cf_chains::Ethereum as Chain>::ChainAsset> =
			HashMap::new();

		let bad_event = make_event(VaultEvents::SwapNativeFilter(SwapNativeFilter {
			dst_chain: 1u32,
			dst_address: ethers::types::Bytes::from(vec![0u8; 20]),
			dst_token: 1u32,
			amount: U256::from(100),
			sender: H160::default(),
			cf_parameters: ethers::types::Bytes::from(vec![0xFFu8; 4]), // garbage
		}));

		let good_event =
			make_event(VaultEvents::TransferNativeFailedFilter(TransferNativeFailedFilter {
				recipient: H160::from([1u8; 20]),
				amount: U256::from(100),
			}));

		let result = handle_vault_events::<Runtime, EthereumInstance>(
			&supported_assets,
			vec![bad_event, good_event],
			&"test_block",
		);

		let events = result.expect("should not fail even with a bad event");
		assert_eq!(events.len(), 2);
		assert!(
			matches!(&events[0], EvmVaultContractEvent::VaultDeposit(witness) if matches!(**witness, VaultDepositWitness::Failed { broker_id: None, .. }))
		);
		assert!(
			matches!(events[1], EvmVaultContractEvent::TransferFailed(_)),
			"the valid event should be a TransferFailed"
		);
	}
	fn swap_events(parameters: Vec<u8>, ccm_parameters: Vec<u8>) -> Vec<VaultEvents> {
		vec![
			VaultEvents::SwapNativeFilter(SwapNativeFilter {
				dst_chain: ForeignChain::Ethereum as u32,
				dst_address: vec![1; 20].into(),
				dst_token: cf_primitives::Asset::Eth as u32,
				amount: 100.into(),
				sender: H160::zero(),
				cf_parameters: parameters.clone().into(),
			}),
			VaultEvents::SwapTokenFilter(SwapTokenFilter {
				dst_chain: ForeignChain::Ethereum as u32,
				dst_address: vec![1; 20].into(),
				dst_token: cf_primitives::Asset::Eth as u32,
				src_token: H160::repeat_byte(2),
				amount: 100.into(),
				sender: H160::zero(),
				cf_parameters: parameters.into(),
			}),
			VaultEvents::XcallNativeFilter(XcallNativeFilter {
				dst_chain: ForeignChain::Ethereum as u32,
				dst_address: vec![1; 20].into(),
				dst_token: cf_primitives::Asset::Eth as u32,
				amount: 100.into(),
				sender: H160::zero(),
				cf_parameters: ccm_parameters.clone().into(),
				message: vec![1, 2].into(),
				gas_amount: 1000.into(),
			}),
			VaultEvents::XcallTokenFilter(XcallTokenFilter {
				dst_chain: ForeignChain::Ethereum as u32,
				dst_address: vec![1; 20].into(),
				dst_token: cf_primitives::Asset::Eth as u32,
				src_token: H160::repeat_byte(2),
				amount: 100.into(),
				sender: H160::zero(),
				cf_parameters: ccm_parameters.into(),
				message: vec![1, 2].into(),
				gas_amount: 1000.into(),
			}),
		]
	}

	fn assert_failed_events<T, I>(token: <T::TargetChain as Chain>::ChainAsset)
	where
		T: pallet_cf_ingress_egress::Config<
			I,
			TargetChain: EvmChain,
			AccountId = cf_primitives::AccountId,
		>,
		I: 'static,
	{
		let tx_hash = H256::repeat_byte(7);
		let events = swap_events(vec![255], vec![255])
			.into_iter()
			.map(|event_parameters| Event { tx_hash, log_index: 0.into(), event_parameters })
			.collect();
		let mapped = handle_vault_events::<T, I>(
			&HashMap::from([(H160::repeat_byte(2), token)]),
			events,
			&42,
		)
		.unwrap();
		assert_eq!(mapped.len(), 4);
		for (index, event) in mapped.into_iter().enumerate() {
			let EvmVaultContractEvent::VaultDeposit(witness) = event else {
				panic!("expected vault deposit")
			};
			let VaultDepositWitness::Failed { deposit, broker_id } = *witness else {
				panic!("expected failed witness")
			};
			assert_eq!(broker_id, None);
			assert_eq!(deposit.deposit_amount, 100);
			assert_eq!(deposit.tx_id, tx_hash);
			assert_eq!(deposit.deposit_details, DepositDetails { tx_hashes: Some(vec![tx_hash]) });
			assert_eq!(
				deposit.input_asset,
				if index % 2 == 0 { T::TargetChain::GAS_ASSET } else { token }
			);
		}
	}

	#[test]
	fn malformed_payloads_preserve_facts_on_every_evm_chain() {
		assert_failed_events::<Runtime, EthereumInstance>(cf_chains::assets::eth::Asset::Usdc);
		assert_failed_events::<Runtime, state_chain_runtime::ArbitrumInstance>(
			cf_chains::assets::arb::Asset::ArbUsdc,
		);
		assert_failed_events::<Runtime, state_chain_runtime::BscInstance>(
			cf_chains::assets::bsc::Asset::BscUsdt,
		);
	}

	fn parameters<C: codec::Encode>(ccm_additional_data: C) -> Vec<u8> {
		use cf_chains::{
			cf_parameters::{CfParametersV1, VersionedCfParameters},
			ChannelRefundParameters,
		};
		codec::Encode::encode(&VersionedCfParameters::V1(CfParametersV1 {
			ccm_additional_data,
			vault_swap_parameters: VaultSwapParametersV1 {
				refund_params: ChannelRefundParameters {
					retry_duration: 5,
					refund_address: H160::repeat_byte(3),
					min_price: Default::default(),
					refund_ccm_metadata: None,
					max_oracle_price_slippage: None,
				},
				dca_params: None,
				boost_fee: 0,
				broker_fee: cf_primitives::Beneficiary {
					account: cf_primitives::AccountId::new([9; 32]),
					bps: 0,
				},
				affiliate_fees: Default::default(),
			},
		}))
	}

	#[test]
	fn valid_payloads_still_produce_swaps() {
		let events =
			swap_events(parameters(()), parameters(cf_chains::CcmAdditionalData::default()))
				.into_iter()
				.map(make_event)
				.collect();
		let mapped = handle_vault_events::<Runtime, EthereumInstance>(
			&HashMap::from([(H160::repeat_byte(2), cf_chains::assets::eth::Asset::Usdc)]),
			events,
			&42,
		)
		.unwrap();
		assert_eq!(mapped.len(), 4);
		assert!(mapped.iter().all(|event|matches!(event,EvmVaultContractEvent::VaultDeposit(witness) if matches!(**witness,VaultDepositWitness::Swap{..}))));
	}

	#[test]
	fn unknown_tokens_and_overflowing_amounts_are_not_witnessed() {
		let mut events = swap_events(vec![255], vec![255]);
		for event in &mut events {
			match event {
				VaultEvents::SwapNativeFilter(event) => event.amount = U256::MAX,
				VaultEvents::XcallNativeFilter(event) => event.amount = U256::MAX,
				_ => {},
			}
		}
		assert!(handle_vault_events::<Runtime, EthereumInstance>(
			&HashMap::new(),
			events.into_iter().map(make_event).collect(),
			&42
		)
		.unwrap()
		.is_empty());
	}

	#[test]
	fn invalid_output_fields_and_ccm_produce_failed_witnesses() {
		let mut events =
			swap_events(parameters(()), parameters(cf_chains::CcmAdditionalData::default()));
		for event in &mut events {
			match event {
				VaultEvents::SwapNativeFilter(event) => event.dst_chain = u32::MAX,
				VaultEvents::SwapTokenFilter(event) => event.dst_token = u32::MAX,
				VaultEvents::XcallNativeFilter(event) => event.message = vec![0; 20000].into(),
				VaultEvents::XcallTokenFilter(event) => event.gas_amount = U256::MAX,
				_ => {},
			}
		}
		let mapped = handle_vault_events::<Runtime, EthereumInstance>(
			&HashMap::from([(H160::repeat_byte(2), cf_chains::assets::eth::Asset::Usdc)]),
			events.into_iter().map(make_event).collect(),
			&42,
		)
		.unwrap();
		assert_eq!(mapped.len(), 4);
		assert!(mapped.iter().all(|event|matches!(event,EvmVaultContractEvent::VaultDeposit(witness) if matches!(**witness,VaultDepositWitness::Failed{broker_id:None,..}))));
	}
	#[test]
	fn failed_witness_identity_ignores_log_position_and_preserves_distinct_amounts() {
		let event = swap_events(vec![255], vec![255]).remove(0);
		let mut changed_amount = event.clone();
		if let VaultEvents::SwapNativeFilter(event) = &mut changed_amount {
			event.amount = 101.into();
		}
		let events = vec![
			Event {
				tx_hash: H256::repeat_byte(7),
				log_index: 0.into(),
				event_parameters: event.clone(),
			},
			Event { tx_hash: H256::repeat_byte(7), log_index: 10.into(), event_parameters: event },
			Event {
				tx_hash: H256::repeat_byte(7),
				log_index: 11.into(),
				event_parameters: changed_amount,
			},
		];
		let witnesses =
			handle_vault_events::<Runtime, EthereumInstance>(&HashMap::new(), events, &42)
				.unwrap()
				.into_iter()
				.map(|event| match event {
					EvmVaultContractEvent::VaultDeposit(witness) => *witness,
					_ => panic!("expected deposit"),
				})
				.collect_vec();
		assert_eq!(witnesses[0], witnesses[1]);
		assert_ne!(witnesses[0], witnesses[2]);
	}
}
