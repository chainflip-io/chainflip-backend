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

// Refund stuck deposits that were witnessed but marked Unrefundable due to
// missing vault metadata. Schedules a separate egress per (asset, deposit) to
// the recipient address provided by the depositor.

use crate::*;
use cf_chains::{
	assets::{arb::Asset as ArbAsset, btc::Asset as BtcAsset, eth::Asset as EthAsset},
	btc::{BitcoinNetwork, ScriptPubkey},
};
use cf_traits::EgressApi;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use hex_literal::hex;
#[cfg(feature = "try-runtime")]
use pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer;
use sp_core::H160;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

const BTC_REFUNDS: &[(u64, &str)] = &[
	// tx: 5268eafc36767ca93977408b664d56a83fc7fd3c03d1a738050ed50cced8b9b7  (0.04454653 BTC)
	(4_454_653, "bc1qgqez4lvdm8xcgj3yyqjlygqu26ggdxqcc69p0e"),
	// tx: ca45345efe101cb384f80629ed0c589272a275aec02ad18704985cba126d5f40  (0.05849854 BTC)
	(5_849_854, "bc1qmrzsdy9du08ltvvhnlkxa7k2vlv8w5he7cm3ca"),
	// tx: 9167b6a64997ac9230e77ec2a895019fd594e62507ae7816a29ae166e07725a2  (2.00906185 BTC)
	(200_906_185, "12kRwfbrxsTVrdboraDLaTRMrj9d1ECZR3"),
	// tx: d7645346f8d9d2c3f86c908cd29d1ca986d7b259413d91548e5b32626d71184f  (0.00096620 BTC)
	(96_620, "1H1Tpn84hLt7DnLxp8WrAhFSYqcV3bc7TY"),
	// tx: 1b817d40c7781884940f018efb5afe318552397b052929ad87bebc88ed053793  (0.12704147 BTC)
	(12_704_147, "1GhSg7G1H2h8Z9eojVwRumrnVFW2q5Ha9d"),
	// tx: 51f011b5262900412569eeb913b0f664a72664a72917ae63366b665f99762a15  (0.52218154 BTC)
	(52_218_154, "bc1q06lrvkcy8ze0qlq65rmfc7xy9xmnpjfn7pf0u6"),
];

const ETH_REFUNDS: &[(EthAsset, u128, [u8; 20])] = &[
	// tx: 0x1b9837d737a2cbf2a5e7243c3bd96f445d9b22716e9b0f687c2eab93d05cef22  (0.44334526 ETH)
	(EthAsset::Eth, 443_345_260_000_000_000, hex!("c0b6ac282641843e89c43519907b481daf84db41")),
	// tx: 0x8590a443441ca8c7a239e21d5ee1385beddebaa2356470fdfbc21f05029f1b91  (2,321.626892 USDC)
	(EthAsset::Usdc, 2_321_626_892, hex!("4a015bde54c592c0ca3fa04838c024ec73141b97")),
	// tx: 0x539b114f19ad4e16a5384b211e2255961216a6d520707d37d601b2e258eca584  (6,000 USDC)
	(EthAsset::Usdc, 6_000_000_000, hex!("2eb3ef36c2024fd2754a6553c0895814ba05a484")),
	// tx: 0x371d422b0afbd063ef2e9f7b525fcf85d29efd6402e4bd51ed5440d68be315cc  (30 USDT)
	(EthAsset::Usdt, 30_000_000, hex!("b59701f007c1e82b9296bd30287650dc19e8f3e6")),
	// tx: 0x02de6ffd14f00e68594da906384f968e179a665422f230c88ca847cdcf799e01  (17,602.230865 USDT)
	(EthAsset::Usdt, 17_602_230_865, hex!("6fcafd9630f35cb0452d9dc5a18a98065e558b1b")),
	// tx: 0xcb5536a5e659a22de0aa915ba1ae3ef70cc2a521049a8f13be4886c5f8f5d4a3  (4,646.379279 USDC)
	(EthAsset::Usdc, 4_646_379_279, hex!("c3a490b6c91a2464eb30f19db4289ac81be1f863")),
];

const ARB_REFUNDS: &[(ArbAsset, u128, [u8; 20])] = &[
	// tx: 0x622b7f62cc072569eabac69d5d87f057a3c31479bf19a45cdf8bd65bdfdcc39a  (19.82018 USDC)
	(ArbAsset::ArbUsdc, 19_820_180, hex!("846aaa63b06e302f86f81d3313cd13df624655f0")),
];

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		for (amount, address) in BTC_REFUNDS {
			let destination =
				match ScriptPubkey::try_from_address(address, &BitcoinNetwork::Mainnet) {
					Ok(spk) => spk,
					Err(e) => {
						log::error!("🪙 Failed to parse BTC address {}: {:?}", address, e);
						continue;
					},
				};
			match <BitcoinIngressEgress as EgressApi<_>>::schedule_egress(
				BtcAsset::Btc,
				*amount,
				destination,
				None,
			) {
				Ok(d) => log::info!(
					"🪙 BTC refund {} sats -> {}: egress_id={:?} after_fees={} fee={}",
					amount,
					address,
					d.egress_id,
					d.egress_amount,
					d.fee_withheld,
				),
				Err(e) => log::error!("🪙 Failed to schedule BTC refund to {}: {:?}", address, e),
			}
		}

		for (asset, amount, address) in ETH_REFUNDS {
			match <EthereumIngressEgress as EgressApi<_>>::schedule_egress(
				*asset,
				*amount,
				H160(*address),
				None,
			) {
				Ok(d) => log::info!(
					"💎 ETH refund {} {:?} -> 0x{}: egress_id={:?} after_fees={} fee={}",
					amount,
					asset,
					hex::encode(address),
					d.egress_id,
					d.egress_amount,
					d.fee_withheld,
				),
				Err(e) => log::error!(
					"💎 Failed to schedule ETH refund {:?} to 0x{}: {:?}",
					asset,
					hex::encode(address),
					e,
				),
			}
		}

		for (asset, amount, address) in ARB_REFUNDS {
			match <ArbitrumIngressEgress as EgressApi<_>>::schedule_egress(
				*asset,
				*amount,
				H160(*address),
				None,
			) {
				Ok(d) => log::info!(
					"🅰️  ARB refund {} {:?} -> 0x{}: egress_id={:?} after_fees={} fee={}",
					amount,
					asset,
					hex::encode(address),
					d.egress_id,
					d.egress_amount,
					d.fee_withheld,
				),
				Err(e) => log::error!(
					"🅰️  Failed to schedule ARB refund {:?} to 0x{}: {:?}",
					asset,
					hex::encode(address),
					e,
				),
			}
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		// Sanity check: every BTC address parses on mainnet.
		for (_, address) in BTC_REFUNDS {
			ScriptPubkey::try_from_address(address, &BitcoinNetwork::Mainnet)
				.map_err(|_| DispatchError::Other("invalid BTC refund address"))?;
		}

		let btc = ScheduledEgressFetchOrTransfer::<Runtime, BitcoinInstance>::decode_len()
			.unwrap_or(0) as u32;
		let eth = ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::decode_len()
			.unwrap_or(0) as u32;
		let arb = ScheduledEgressFetchOrTransfer::<Runtime, ArbitrumInstance>::decode_len()
			.unwrap_or(0) as u32;
		let mut buf = Vec::with_capacity(12);
		buf.extend_from_slice(&btc.to_be_bytes());
		buf.extend_from_slice(&eth.to_be_bytes());
		buf.extend_from_slice(&arb.to_be_bytes());
		Ok(buf)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		if state.len() != 12 {
			return Err(DispatchError::Other("bad pre_upgrade state"));
		}
		let btc_before = u32::from_be_bytes(state[0..4].try_into().unwrap());
		let eth_before = u32::from_be_bytes(state[4..8].try_into().unwrap());
		let arb_before = u32::from_be_bytes(state[8..12].try_into().unwrap());
		let btc_after = ScheduledEgressFetchOrTransfer::<Runtime, BitcoinInstance>::decode_len()
			.unwrap_or(0) as u32;
		let eth_after = ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::decode_len()
			.unwrap_or(0) as u32;
		let arb_after = ScheduledEgressFetchOrTransfer::<Runtime, ArbitrumInstance>::decode_len()
			.unwrap_or(0) as u32;
		assert_eq!(
			btc_after,
			btc_before + BTC_REFUNDS.len() as u32,
			"unexpected BTC egress queue delta",
		);
		assert_eq!(
			eth_after,
			eth_before + ETH_REFUNDS.len() as u32,
			"unexpected Ethereum egress queue delta",
		);
		assert_eq!(
			arb_after,
			arb_before + ARB_REFUNDS.len() as u32,
			"unexpected Arbitrum egress queue delta",
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn btc_refund_addresses_parse_on_mainnet() {
		for (amount, address) in BTC_REFUNDS {
			ScriptPubkey::try_from_address(address, &BitcoinNetwork::Mainnet).unwrap_or_else(|e| {
				panic!(
					"BTC refund address {} (amount {}) failed to parse: {:?}",
					address, amount, e
				)
			});
		}
	}
}
