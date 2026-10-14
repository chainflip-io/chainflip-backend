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

//! One-shot Berghain recovery of failed deposits.
//!
//! Each deposit is returned to an address recorded on chain for it: the deposit channel's
//! registered return address, or the address the deposit was sent from. All transfers are paid
//! from the vault, and pay the normal egress fee.
//!
//! Also sweeps into the vault:
//!
//! - Expired Solana deposit channels that still hold funds. A queued `Fetch` is only dispatched
//!   while its channel is in `DepositChannelLookup`, so any queued `Fetch` for these channels is
//!   replaced by a fetch broadcast directly.
//! - Ethereum and Tron deposit channel addresses that hold funds but whose `Deposit` contract was
//!   never deployed. Deploying the contract with the channel's asset puts it at the channel address
//!   and sweeps that asset. Any other asset at the address is fetched from the new contract in the
//!   same `allBatch`, which runs its deploys before its fetches.

use crate::{
	chainflip::{address_derivation::AddressDerivation, EvmEnvironment, SolEnvironment},
	*,
};
use cf_chains::{
	address::AddressDerivationApi,
	assets::{
		btc::Asset as BtcAsset, eth::Asset as EthAsset, hub::Asset as HubAsset,
		sol::Asset as SolAsset, tron::Asset as TronAsset,
	},
	btc::ScriptPubkey,
	dot::PolkadotAccountId,
	eth::api::EthereumApi,
	evm::EvmFetchId,
	sol::{api::SolanaApi, SolAddress, SolanaDepositFetchId},
	tron::api::TronApi,
	AllBatch, Assethub, Bitcoin, Chain, Ethereum, FetchAssetParams, Solana, Tron,
};
use cf_primitives::ChannelId;
use cf_runtime_utilities::genesis_hashes;
use cf_traits::{Broadcaster, EgressApi};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use hex_literal::hex;
use pallet_cf_ingress_egress::{FetchOrTransfer, ScheduledEgressFetchOrTransfer};
use sp_core::H160;
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

// One-shot gate. Must equal the runtime's spec_version at the moment the migration ships, so a
// release that forgets to remove it cannot pay them out twice. Update this in lock-step with
// VERSION.spec_version, and remove this module in the release that follows.
const RECOVERY_SPEC_VERSION: u32 = 2_02_22;

/// A failed deposit, and the address recorded on chain that it is returned to.
struct Recovery<C: Chain> {
	/// The ticket, and the deposit transaction or extrinsic.
	case: &'static str,
	asset: C::ChainAsset,
	/// What the vault holds for the deposit.
	amount: C::ChainAmount,
	destination: C::ChainAccount,
}

/// Deposits to swap channels that were never witnessed while Assethub chain tracking was stalled.
/// Each was swept into the vault, unwitnessed, by a later fetch from its reused channel address.
/// Returned to the channel's registered return address. The channel ids and deposit addresses are
/// checked in the tests.
const HUB_RECOVERIES: &[Recovery<Assethub>] = &[
	// 2,000 DOT to channel 8 (146E3dzGDXhXfv4tt4miv3RHjEdLXFUzxCmqUr8fp6dPse1Y).
	// Return address 15DQWnuLk3QbeYnUE7R5d35u3JXgPt6VxoJtZxa2xYHcQZue.
	Recovery {
		case: "COM-570, 21309573-2",
		asset: HubAsset::HubDot,
		amount: 20_000_000_000_000,
		destination: PolkadotAccountId::from_aliased(hex!(
			"ba67085bbd451dbe92af133bea17b84d5b14537d2cba1726d31cf7f0e701c25a"
		)),
	},
	// 2,996 DOT to channel 42 (182gdBvK8sgzRgA2PwDbrKg2M9WoLvXFn2h2PZim7qoSinr).
	// Return address 1vUTPxYuCij56BguDg73wuk5tXujb5BSZHkmQU5S7Jdg8Zw.
	Recovery {
		case: "COM-571, 21309894-2",
		asset: HubAsset::HubDot,
		amount: 29_960_000_000_000,
		destination: PolkadotAccountId::from_aliased(hex!(
			"28c86db4a5faaf04da374749e22b5303eecdd0149140a42362b7ddce84cb9e29"
		)),
	},
	// 1,800 DOT to channel 49 (14CPPi1QvzDQxC5mX29uj1Q55kUGX7P3SenYkkzjQqSir8Pq), from the same
	// depositor as channel 8. Return address 15DQWnuLk3QbeYnUE7R5d35u3JXgPt6VxoJtZxa2xYHcQZue.
	Recovery {
		case: "COM-588, 21334662-2",
		asset: HubAsset::HubDot,
		amount: 18_000_000_000_000,
		destination: PolkadotAccountId::from_aliased(hex!(
			"ba67085bbd451dbe92af133bea17b84d5b14537d2cba1726d31cf7f0e701c25a"
		)),
	},
];

/// 1,000 USDT sent to a USDC deposit channel at 0xf6a186e3adcc998669b01532ae799030a4076687. It was
/// swept into the vault, unwitnessed, by the fetch (0x6fcc479f…) for an 8,001 USDT deposit to a
/// later channel at the same address, which fetched 9,001 USDT. Returned to the sender.
const ETH_RECOVERIES: &[Recovery<Ethereum>] = &[Recovery {
	case: "COM-593, 0xb287b87e30511445000b317d639a4bc5898b3d2373ed2c45dc44fe9e3a225c0e",
	asset: EthAsset::Usdt,
	amount: 1_000_000_000,
	destination: H160(hex!("2f4bbdab4a0b08d0020ff295485b70e6de19654c")),
}];

/// 4,192,000 sats sent to private channel 78124 by a transaction whose OP_RETURN was dropped when
/// it was replaced by fee (RBF), and so ingressed without a swap or a return address. Returned to
/// the sender, bc1qkl33q06z09ycsys6cqtnsrd6fl6uk9uupe9fgr, less the 424 sats ingress fee already
/// withheld.
const BTC_RECOVERIES: &[Recovery<Bitcoin>] = &[Recovery {
	case: "COM-608, 2a916919275155cd4fd995214f85e54b93bab3ffcd799e1fa2d125c2a3add3ef",
	asset: BtcAsset::Btc,
	amount: 4_191_576,
	destination: ScriptPubkey::P2WPKH(hex!("b7e3103f42794988121ac017380dba4ff5cb179c")),
}];

/// 1.619 SOL sent to channel 139141 within its lifetime, but witnessed after it had expired, and
/// so rejected with `InvalidDepositAddress`. The deposit is still at the channel address, which is
/// swept below. Returned to the channel's registered return address, which is also the sender,
/// AUjqF8GUVL6v5YfzugbkHWQdPpBi2JFAq3ZFBT4evyqH.
const SOL_RECOVERIES: &[Recovery<Solana>] = &[Recovery {
	case: "COM-603, 3NWE8XHmHbY3qZAd3sbrXDvy397Bt3k7wEC1UGvNxx5bB5Mzk8RnPaAuucTLxWXD9Kibk2fuSNoaEbpArC6JaEok",
	asset: SolAsset::Sol,
	amount: 1_619_000_000,
	destination: SolAddress(hex!(
		"8cd44b530e41997754f71ea19af77fb705455c72f30109213e674ea775d0cac8"
	)),
}];

/// (channel id, channel address) of the expired Solana channels to sweep. Each address is
/// re-derived from its channel id, and the channel is skipped if the two disagree.
const SOL_CHANNEL_SWEEPS: &[(ChannelId, [u8; 32])] = &[
	// COM-371: 9YJGWpYPdpGX5Yn3ycC5KuJRPTcsWehcw4Vk1baY1sYv. Its deposit was already returned from
	// the vault in 2.2.13.
	(118_758, hex!("7ee2065fd412020955e3e1d849579ea974bfc79493833d45d8ffc0a93e1a3a53")),
	// COM-603: 9nue7z71iKXBLhSyEAgxZxuF1SqDVCehTgKdGnsFD6eu, returned above.
	(139_141, hex!("82a026edbeb40027caa216467f0f4a438dc9065f0a5ebd80647167b3d7ad4f0a")),
];

/// (channel id, asset the channel address derives from, channel address, other assets to fetch)
type UndeployedChannel<A> = (ChannelId, A, [u8; 20], &'static [A]);

const ETH_UNDEPLOYED_CHANNELS: &[UndeployedChannel<EthAsset>] = &[
	// 142 USDC on a USDT channel.
	(2037, EthAsset::Usdt, hex!("e618c90a4e027bf04c7c14161920b25258586717"), &[EthAsset::Usdc]),
];

/// Each address holds the other asset to the one it derives from.
const TRON_UNDEPLOYED_CHANNELS: &[UndeployedChannel<TronAsset>] = &[
	// 220.829575 USDT on a TRX channel at TLgzrt5zr2jm1YWAkw8FQSfGnH6ySvEPF2.
	(16, TronAsset::Trx, hex!("759897ec0890f2267264ad5e1d9ef440b92cf15f"), &[TronAsset::TrxUsdt]),
	// 100.000001 TRX on a USDT channel at TSRvw1cLuyS2eLVfCtFupkAVGXSxMGMymb.
	(26, TronAsset::TrxUsdt, hex!("b48fd4419477c57b7766876c45d858539ff3bbe9"), &[TronAsset::Trx]),
];

fn is_swept_sol_channel_fetch(request: &FetchOrTransfer<Solana>) -> bool {
	matches!(
		request,
		FetchOrTransfer::Fetch { deposit_address, .. }
			if SOL_CHANNEL_SWEEPS.iter().any(|(_, address)| *deposit_address == SolAddress(*address))
	)
}

#[cfg(feature = "try-runtime")]
fn queued_swept_sol_channel_fetches() -> usize {
	ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::get()
		.iter()
		.filter(|request| is_swept_sol_channel_fetch(request))
		.count()
}

/// The transfers queued on Assethub, Ethereum, Bitcoin and Solana.
#[cfg(feature = "try-runtime")]
fn queued_transfers() -> [u32; 4] {
	fn transfers<C: Chain>(queue: Vec<FetchOrTransfer<C>>) -> u32 {
		queue
			.iter()
			.filter(|request| matches!(request, FetchOrTransfer::Transfer { .. }))
			.count() as u32
	}
	[
		transfers(ScheduledEgressFetchOrTransfer::<Runtime, AssethubInstance>::get()),
		transfers(ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::get()),
		transfers(ScheduledEgressFetchOrTransfer::<Runtime, BitcoinInstance>::get()),
		transfers(ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::get()),
	]
}

/// One transfer per recovered deposit.
#[cfg(feature = "try-runtime")]
const EXPECTED_TRANSFERS: [u32; 4] = [
	HUB_RECOVERIES.len() as u32,
	ETH_RECOVERIES.len() as u32,
	BTC_RECOVERIES.len() as u32,
	SOL_RECOVERIES.len() as u32,
];

#[cfg(feature = "try-runtime")]
const TRANSFER_CHAIN_NAMES: [&str; 4] = ["Assethub", "Ethereum", "Bitcoin", "Solana"];

/// Each sweep is a single broadcast, on Solana, Ethereum and Tron, with one signing request each.
#[cfg(feature = "try-runtime")]
fn broadcast_counters() -> [u32; 4] {
	[
		pallet_cf_broadcast::BroadcastIdCounter::<Runtime, SolanaInstance>::get(),
		pallet_cf_broadcast::BroadcastIdCounter::<Runtime, EthereumInstance>::get(),
		pallet_cf_broadcast::BroadcastIdCounter::<Runtime, TronInstance>::get(),
		pallet_cf_cfe_interface::RuntimeUpgradeEvents::<Runtime>::decode_len().unwrap_or(0) as u32,
	]
}

#[cfg(feature = "try-runtime")]
const BROADCAST_CHAIN_NAMES: [&str; 3] = ["Solana", "Ethereum", "Tron"];

fn is_active() -> bool {
	matches!(genesis_hashes::genesis_hash::<Runtime>(), genesis_hashes::BERGHAIN) &&
		VERSION.spec_version == RECOVERY_SPEC_VERSION
}

fn schedule_recoveries() {
	let earlier_cfe_events =
		pallet_cf_cfe_interface::CfeEvents::<Runtime>::decode_len().unwrap_or(0);

	return_deposits::<Assethub, AssethubIngressEgress>(HUB_RECOVERIES);
	return_deposits::<Ethereum, EthereumIngressEgress>(ETH_RECOVERIES);
	return_deposits::<Bitcoin, BitcoinIngressEgress>(BTC_RECOVERIES);
	return_deposits::<Solana, SolanaIngressEgress>(SOL_RECOVERIES);
	sweep_sol_channels();
	deploy_channels::<Ethereum, EthereumApi<EvmEnvironment>, EthereumBroadcaster>(
		ETH_UNDEPLOYED_CHANNELS,
	);
	deploy_channels::<Tron, TronApi<EvmEnvironment>, TronBroadcaster>(TRON_UNDEPLOYED_CHANNELS);

	// `CfeEvents` is cleared in `on_initialize`, which runs after `on_runtime_upgrade`, so the
	// signing requests are stashed for it to re-instate. Earlier events were already observed in
	// the previous block, and are left to be cleared.
	let signing_requests = pallet_cf_cfe_interface::CfeEvents::<Runtime>::mutate(|events| {
		events.split_off(earlier_cfe_events.min(events.len()))
	});
	pallet_cf_cfe_interface::RuntimeUpgradeEvents::<Runtime>::mutate(|events| {
		events.extend(signing_requests)
	});
}

fn return_deposits<C: Chain, E: EgressApi<C>>(recoveries: &[Recovery<C>]) {
	for Recovery { case, asset, amount, destination } in recoveries {
		match E::schedule_egress(*asset, *amount, destination.clone(), None) {
			Ok(d) => log::info!(
				"💸 {case}: returning {amount:?} {asset:?}: egress_id={:?} after_fees={:?} fee={:?}",
				d.egress_id,
				d.egress_amount,
				d.fee_withheld,
			),
			Err(e) => log::error!(
				"💸 {case}: failed to schedule the {} transfer: {:?}",
				C::NAME,
				Into::<DispatchError>::into(e),
			),
		}
	}
}

/// Broadcasts a single `allBatch` that deploys each channel's `Deposit` contract, and fetches any
/// other assets from it. Channels whose address does not derive from their id are skipped.
fn deploy_channels<C, Api, B>(channels: &[UndeployedChannel<C::ChainAsset>])
where
	C: Chain<ChainAccount = H160, DepositFetchId = EvmFetchId>,
	AddressDerivation: AddressDerivationApi<C>,
	Api: AllBatch<C>,
	B: Broadcaster<C, ApiCall = Api>,
{
	let mut fetch_params = Vec::new();
	for (channel_id, asset, address, other_assets) in channels {
		match <AddressDerivation as AddressDerivationApi<C>>::generate_address(*asset, *channel_id)
		{
			Ok(derived) if derived == H160(*address) => {
				fetch_params.push(FetchAssetParams {
					deposit_fetch_id: EvmFetchId::DeployAndFetch(*channel_id),
					asset: *asset,
				});
				fetch_params.extend(other_assets.iter().map(|other| FetchAssetParams {
					deposit_fetch_id: EvmFetchId::Fetch(derived),
					asset: *other,
				}));
			},
			other => log::error!(
				"💸 {} channel {channel_id}: derived {other:?}, expected 0x{}; not deploying.",
				C::NAME,
				hex::encode(address),
			),
		}
	}

	if fetch_params.is_empty() {
		return;
	}

	match Api::new_unsigned(fetch_params, Vec::new()) {
		Ok(calls) =>
			for (call, _) in calls {
				let (broadcast_id, _) = B::threshold_sign_and_broadcast(call);
				log::info!(
					"💸 {} channels {:?}: deploy broadcast_id={broadcast_id}.",
					C::NAME,
					channels.iter().map(|(channel_id, ..)| channel_id).collect::<Vec<_>>(),
				);
			},
		Err(e) => log::error!("💸 {}: failed to build the deploy batch: {e:?}", C::NAME),
	}
}

/// Broadcasts a single `allBatch` that fetches from each channel, and drops any queued `Fetch` for
/// them. Channels whose address does not derive from their id are skipped.
fn sweep_sol_channels() {
	let mut fetch_params = Vec::new();
	for (channel_id, expected) in SOL_CHANNEL_SWEEPS {
		match <AddressDerivation as AddressDerivationApi<Solana>>::generate_address_and_state(
			SolAsset::Sol,
			*channel_id,
		) {
			Ok((address, bump)) if address == SolAddress(*expected) =>
				fetch_params.push(FetchAssetParams {
					deposit_fetch_id: SolanaDepositFetchId {
						channel_id: *channel_id,
						address,
						bump,
					},
					asset: SolAsset::Sol,
				}),
			other => log::error!(
				"💸 Solana channel {channel_id}: derived {other:?}, expected {:?}; not sweeping.",
				SolAddress(*expected),
			),
		}
	}

	if fetch_params.is_empty() {
		return;
	}

	match <SolanaApi<SolEnvironment> as AllBatch<Solana>>::new_unsigned(fetch_params, vec![]) {
		Ok(calls) => {
			for (call, _) in calls {
				let (broadcast_id, _) =
					<SolanaBroadcaster as Broadcaster<Solana>>::threshold_sign_and_broadcast(call);
				log::info!(
					"💸 Solana channels {:?}: fetch broadcast_id={broadcast_id}.",
					SOL_CHANNEL_SWEEPS.iter().map(|(channel_id, _)| channel_id).collect::<Vec<_>>(),
				);
			}
			ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::mutate(|queue| {
				queue.retain(|request| !is_swept_sol_channel_fetch(request))
			});
		},
		Err(e) => log::error!("💸 Solana: failed to build the fetch batch: {e:?}"),
	}
}

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		if is_active() {
			schedule_recoveries();
			log::info!("💸 Berghain: scheduled fund recovery.");
		} else {
			log::info!(
				"💸 Skipping fund recovery: not Berghain, or spec_version is {} (expected {}).",
				VERSION.spec_version,
				RECOVERY_SPEC_VERSION,
			);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(if is_active() {
			queued_transfers()
				.into_iter()
				.chain(broadcast_counters())
				.flat_map(|n| n.to_be_bytes())
				.collect()
		} else {
			Default::default()
		})
	}

	/// A transfer that failed to schedule shows up as a queue delta mismatch.
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		if !is_active() {
			return Ok(());
		}

		let before = state
			.chunks_exact(4)
			.map(|chunk| chunk.try_into().map(u32::from_be_bytes))
			.collect::<Result<Vec<_>, _>>()
			.map_err(|_| DispatchError::Other("bad pre_upgrade state"))?;
		if before.len() != 8 {
			return Err(DispatchError::Other("bad pre_upgrade state"));
		}

		for (i, after) in queued_transfers().into_iter().enumerate() {
			assert_eq!(
				after,
				before[i] + EXPECTED_TRANSFERS[i],
				"expected one {} transfer per recovered deposit",
				TRANSFER_CHAIN_NAMES[i],
			);
		}

		let [sol, eth, tron, stashed_events] = broadcast_counters();
		for (i, after) in [sol, eth, tron].into_iter().enumerate() {
			assert_eq!(
				after,
				before[4 + i] + 1,
				"expected one {} broadcast",
				BROADCAST_CHAIN_NAMES[i],
			);
		}
		assert_eq!(
			stashed_events,
			before[7] + BROADCAST_CHAIN_NAMES.len() as u32,
			"expected one stashed signing request per broadcast",
		);

		// The Solana sweep replaces any queued fetch for the swept channels.
		assert_eq!(
			queued_swept_sol_channel_fetches(),
			0,
			"queued Solana channel fetch not removed"
		);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cf_chains::{
		btc::{BitcoinFeeInfo, BitcoinNetwork, BitcoinTrackedData},
		eth::EthereumTrackedData,
		hub::{calculate_derived_address, AssethubTrackedData},
		sol::{
			sol_tx_core::{address_derivation::derive_deposit_address, PdaAndBump},
			SolTrackedData,
		},
		ChainState,
	};
	use sp_core::crypto::{AccountId32, Ss58Codec};

	/// (channel id, deposit address, return address) of each of HUB_RECOVERIES.
	const HUB_CHANNELS: [(ChannelId, &str, &str); 3] = [
		(
			8,
			"146E3dzGDXhXfv4tt4miv3RHjEdLXFUzxCmqUr8fp6dPse1Y",
			"15DQWnuLk3QbeYnUE7R5d35u3JXgPt6VxoJtZxa2xYHcQZue",
		),
		(
			42,
			"182gdBvK8sgzRgA2PwDbrKg2M9WoLvXFn2h2PZim7qoSinr",
			"1vUTPxYuCij56BguDg73wuk5tXujb5BSZHkmQU5S7Jdg8Zw",
		),
		(
			49,
			"14CPPi1QvzDQxC5mX29uj1Q55kUGX7P3SenYkkzjQqSir8Pq",
			"15DQWnuLk3QbeYnUE7R5d35u3JXgPt6VxoJtZxa2xYHcQZue",
		),
	];

	fn ss58(account: &PolkadotAccountId) -> String {
		AccountId32::new(*account.aliased_ref()).to_ss58check_with_version(0u16.into())
	}

	/// The deposits went to these channels, and are returned to the addresses they registered.
	#[test]
	fn hub_channels_derive_from_the_berghain_vault() {
		// Environment::AssethubVaultAccountId on Berghain.
		let vault_account = PolkadotAccountId::from_aliased(hex!(
			"60fb53335061493b4c7994814e708f5aacab816cd564ec522be7953aecebc557"
		));
		assert_eq!(HUB_RECOVERIES.len(), HUB_CHANNELS.len());
		for (Recovery { destination, .. }, (channel_id, deposit_address, return_address)) in
			HUB_RECOVERIES.iter().zip(HUB_CHANNELS)
		{
			assert_eq!(
				ss58(&calculate_derived_address(vault_account, channel_id)),
				deposit_address
			);
			assert_eq!(ss58(destination), return_address);
		}
	}

	#[test]
	fn btc_and_sol_addresses_encode_as_expected() {
		assert_eq!(
			BTC_RECOVERIES[0].destination.to_address(&BitcoinNetwork::Mainnet),
			"bc1qkl33q06z09ycsys6cqtnsrd6fl6uk9uupe9fgr"
		);
		assert_eq!(
			SOL_RECOVERIES[0].destination.to_string(),
			"AUjqF8GUVL6v5YfzugbkHWQdPpBi2JFAq3ZFBT4evyqH"
		);
	}

	#[test]
	fn sol_channel_addresses_derive_from_the_berghain_vault_program() {
		// SolanaApiEnvironment::vault_program on Berghain.
		let vault_program =
			SolAddress(hex!("93448d0d660b4cf89feb3486cc8cf39cfb4e8331a0840246ce4357e78c5f13f4"));
		for ((channel_id, expected), encoded) in SOL_CHANNEL_SWEEPS.iter().zip([
			"9YJGWpYPdpGX5Yn3ycC5KuJRPTcsWehcw4Vk1baY1sYv",
			"9nue7z71iKXBLhSyEAgxZxuF1SqDVCehTgKdGnsFD6eu",
		]) {
			let PdaAndBump { address, .. } =
				derive_deposit_address(*channel_id, vault_program).unwrap();
			assert_eq!(address, SolAddress(*expected));
			assert_eq!(address.to_string(), encoded);
		}
	}

	/// Ethereum and Tron vault and token addresses on Berghain.
	#[test]
	fn undeployed_channel_addresses_derive_from_the_berghain_vaults() {
		use cf_chains::eth::deposit_address::{
			get_create_2_address, get_create_2_address_tron, ETHEREUM_ETH_ADDRESS,
		};

		let eth_vault = H160(hex!("f5e10380213880111522dd0efd3dbb45b9f62bcc"));
		let eth_token = |asset| match asset {
			EthAsset::Usdt => H160(hex!("dac17f958d2ee523a2206206994597c13d831ec7")),
			other => panic!("no token address for {other:?}"),
		};
		for (channel_id, asset, address, _) in ETH_UNDEPLOYED_CHANNELS {
			assert_eq!(
				get_create_2_address(eth_vault, Some(eth_token(*asset)), *channel_id),
				H160(*address)
			);
		}

		let tron_vault = H160(hex!("32e07e5dfcb75c2977adddc593eb8d6b6e71f96e"));
		let tron_token = |asset| match asset {
			TronAsset::Trx => ETHEREUM_ETH_ADDRESS,
			TronAsset::TrxUsdt => H160(hex!("a614f803b6fd780986a42c78ec9c7f77e6ded13c")),
		};
		for (channel_id, asset, address, _) in TRON_UNDEPLOYED_CHANNELS {
			assert_eq!(
				get_create_2_address_tron(tron_vault, Some(tron_token(*asset)), *channel_id),
				H160(*address)
			);
		}
	}

	#[test]
	fn tron_channel_addresses_encode_as_expected() {
		for ((_, _, address, _), expected) in TRON_UNDEPLOYED_CHANNELS
			.iter()
			.zip(["TLgzrt5zr2jm1YWAkw8FQSfGnH6ySvEPF2", "TSRvw1cLuyS2eLVfCtFupkAVGXSxMGMymb"])
		{
			assert_eq!(
				cf_chains::tron::TronAddress::from_evm_address(H160(*address)).to_base58check(),
				expected
			);
		}
	}

	fn assert_transfers_queued<C: Chain, I: 'static>(recoveries: &[Recovery<C>])
	where
		Runtime: pallet_cf_ingress_egress::Config<I, TargetChain = C>,
	{
		let queue = ScheduledEgressFetchOrTransfer::<Runtime, I>::get();
		assert_eq!(queue.len(), recoveries.len(), "{}", C::NAME);
		for (request, Recovery { asset, destination, .. }) in queue.iter().zip(recoveries) {
			assert!(
				matches!(
					request,
					FetchOrTransfer::Transfer { asset: queued, destination_address, .. }
						if queued == asset && destination_address == destination
				),
				"{}: unexpected {request:?}",
				C::NAME,
			);
		}
	}

	#[test]
	fn schedules_the_returns() {
		sp_io::TestExternalities::default().execute_with(|| {
			// Set at genesis, and needed for the egress fee estimates.
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::put(
				ChainState {
					block_height: 0,
					tracked_data: AssethubTrackedData {
						median_tip: 0,
						runtime_version: Default::default(),
					},
				},
			);
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, EthereumInstance>::put(
				ChainState {
					block_height: 0,
					tracked_data: EthereumTrackedData { base_fee: 0, priority_fee: 0 },
				},
			);
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::put(
				ChainState {
					block_height: 0,
					tracked_data: BitcoinTrackedData { btc_fee_info: BitcoinFeeInfo::new(10_000) },
				},
			);
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, SolanaInstance>::put(
				ChainState { block_height: 0, tracked_data: SolTrackedData { priority_fee: 0 } },
			);

			return_deposits::<Assethub, AssethubIngressEgress>(HUB_RECOVERIES);
			return_deposits::<Ethereum, EthereumIngressEgress>(ETH_RECOVERIES);
			return_deposits::<Bitcoin, BitcoinIngressEgress>(BTC_RECOVERIES);
			return_deposits::<Solana, SolanaIngressEgress>(SOL_RECOVERIES);

			assert_transfers_queued::<_, AssethubInstance>(HUB_RECOVERIES);
			assert_transfers_queued::<_, EthereumInstance>(ETH_RECOVERIES);
			assert_transfers_queued::<_, BitcoinInstance>(BTC_RECOVERIES);
			assert_transfers_queued::<_, SolanaInstance>(SOL_RECOVERIES);
		});
	}
}
