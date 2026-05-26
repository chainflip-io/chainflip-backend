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

use super::super::common::chunked_chain_source::chunked_by_vault::{
	builder::ChunkedByVaultBuilder, ChunkedByVault,
};
use crate::{
	dot::{
		retry_rpc::{DotRetryRpcApi, DotRetryRpcClient},
		PolkadotHash,
	},
	witness::{
		common::{
			chunked_chain_source::chunked_by_vault::deposit_addresses::Addresses,
			RuntimeCallHasChain, RuntimeHasChain,
		},
		hub::EventWrapper,
	},
};
use cf_chains::{assets::hub::Asset as HubAsset, dot::PolkadotAccountId, Assethub};
use cf_primitives::{
	EpochIndex, PolkadotBlockNumber, ASSETHUB_USDC_ASSET_ID, ASSETHUB_USDT_ASSET_ID,
};
use futures_core::Future;
use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness};
use state_chain_runtime::AssethubInstance;
use std::collections::BTreeMap;
use subxt::events::Phase;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn hub_deposits<ProcessCall, ProcessingFut>(
		self,
		process_call: ProcessCall,
		hub_client: DotRetryRpcClient,
	) -> ChunkedByVaultBuilder<
		impl ChunkedByVault<
			Index = PolkadotBlockNumber,
			Hash = PolkadotHash,
			Data = Vec<(Phase, EventWrapper)>,
			Chain = Assethub,
			ExtraInfo = PolkadotAccountId,
			ExtraHistoricInfo = (),
		>,
	>
	where
		Inner: ChunkedByVault<
			Index = PolkadotBlockNumber,
			Hash = PolkadotHash,
			Data = (Vec<(Phase, EventWrapper)>, Addresses<Inner>),
			Chain = Assethub,
			ExtraInfo = PolkadotAccountId,
			ExtraHistoricInfo = (),
		>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		self.then(move |epoch, header| {
			let process_call = process_call.clone();
			let hub_client = hub_client.clone();
			async move {
				let (events, addresses_and_details) = header.data;

				let addresses = address_and_details_to_addresses(addresses_and_details);

				let deposit_witnesses = deposit_witnesses(
					header.hash,
					header.parent_hash,
					&hub_client,
					addresses,
					&events,
				)
				.await;

				if !deposit_witnesses.is_empty() {
					process_call(
						pallet_cf_ingress_egress::Call::<_, AssethubInstance>::process_deposits {
							deposit_witnesses,
							block_height: header.index,
						}
						.into(),
						epoch.index,
					)
					.await
				}

				events
			}
		})
	}
}

fn address_and_details_to_addresses(
	address_and_details: Vec<DepositChannelDetails<state_chain_runtime::Runtime, AssethubInstance>>,
) -> Vec<PolkadotAccountId> {
	address_and_details
		.into_iter()
		.map(|deposit_channel_details| {
			assert!(
				deposit_channel_details.deposit_channel.asset == HubAsset::HubDot ||
					deposit_channel_details.deposit_channel.asset == HubAsset::HubUsdc ||
					deposit_channel_details.deposit_channel.asset == HubAsset::HubUsdt
			);
			deposit_channel_details.deposit_channel.address
		})
		.collect()
}

async fn balance_increase<Client: DotRetryRpcApi + Send + Sync>(
	hub_client: &Client,
	address: PolkadotAccountId,
	asset: HubAsset,
	block_hash: PolkadotHash,
	parent_hash: Option<PolkadotHash>,
) -> u128 {
	let (current_block_balance, previous_block_balance): (u128, u128) =
		futures::join!(hub_client.liquid_account_balance(address, asset, block_hash), async move {
			if let Some(parent_hash) = parent_hash {
				hub_client.liquid_account_balance(address, asset, parent_hash).await
			} else {
				0
			}
		});
	current_block_balance.saturating_sub(previous_block_balance)
}

// Return the deposit witnesses and the extrinsic indices of transfers we want
// to confirm the broadcast of.
async fn deposit_witnesses<Client: DotRetryRpcApi + Send + Sync>(
	block_hash: PolkadotHash,
	parent_hash: Option<PolkadotHash>,
	hub_client: &Client,
	monitored_addresses: Vec<PolkadotAccountId>,
	events: &Vec<(Phase, EventWrapper)>,
) -> Vec<DepositWitness<Assethub>> {
	// First we check for any outgoing transfers from the monitored addresses (fetches) so
	// we can take these into account when calculating balance changes.
	let mut outgoing_balance_changes = BTreeMap::default();
	for (phase, wrapped_event) in events {
		if let Phase::ApplyExtrinsic(_) = phase {
			match wrapped_event {
				EventWrapper::BalancesTransfer { amount, from, .. } => {
					let from = PolkadotAccountId::from_aliased(from.0);
					if monitored_addresses.contains(&from) {
						*outgoing_balance_changes.entry((from, HubAsset::HubDot)).or_insert(0) +=
							*amount;
					}
				},
				EventWrapper::AssetsTransfer { asset_id, amount, from, .. } => {
					let from = PolkadotAccountId::from_aliased(from.0);
					let asset = match *asset_id {
						ASSETHUB_USDT_ASSET_ID => HubAsset::HubUsdt,
						ASSETHUB_USDC_ASSET_ID => HubAsset::HubUsdc,
						_ => continue,
					};
					if monitored_addresses.contains(&from) {
						*outgoing_balance_changes.entry((from, asset)).or_insert(0) += *amount;
					}
				},
				_ => continue,
			}
		}
	}
	// We track the available liquid balance for each address and asset as we go through the events,
	// so that if there are multiple transfers to the same address in the same block, we can
	// correctly witness them up to the amount of liquid balance available. Tracking this prevents
	// a scenario is necessary to correctly handle two specific edge cases:
	// 1) Transfer of vesting funds: for this reason, we need track *liquid* balances changes.
	// 2) Multiple transfers in the same block to the same address: without tracking liquid balance
	//    changes.
	let mut deposit_witnesses = vec![];
	let mut available_liquid_balances = BTreeMap::default();
	for (phase, wrapped_event) in events {
		if let Phase::ApplyExtrinsic(extrinsic_index) = phase {
			let (asset, deposit_address, amount) = match wrapped_event {
				EventWrapper::BalancesTransfer { to, amount, from: _ } => {
					let deposit_address = PolkadotAccountId::from_aliased(to.0);
					if !monitored_addresses.contains(&deposit_address) {
						continue;
					} else {
						(HubAsset::HubDot, deposit_address, *amount)
					}
				},
				EventWrapper::AssetsTransfer { asset_id, to, amount, from: _ } => {
					let deposit_address = PolkadotAccountId::from_aliased(to.0);
					if !monitored_addresses.contains(&deposit_address) {
						continue;
					} else {
						(
							match *asset_id {
								ASSETHUB_USDT_ASSET_ID => HubAsset::HubUsdt,
								ASSETHUB_USDC_ASSET_ID => HubAsset::HubUsdc,
								_ => continue,
							},
							deposit_address,
							*amount,
						)
					}
				},
				_ => continue,
			};
			let available_balance = if let Some(balance) =
				available_liquid_balances.get(&(deposit_address, asset))
			{
				*balance
			} else {
				let balance =
					balance_increase(hub_client, deposit_address, asset, block_hash, parent_hash)
						.await
						.saturating_add(
							outgoing_balance_changes
								.get(&(deposit_address, asset))
								.copied()
								.unwrap_or_default(),
						);
				available_liquid_balances.insert((deposit_address, asset), balance);
				balance
			};
			let amount = amount.min(available_balance);
			available_liquid_balances
				.entry((deposit_address, asset))
				.and_modify(|balance| *balance = balance.saturating_sub(amount));
			if amount > 0 {
				deposit_witnesses.push(DepositWitness {
					deposit_address,
					asset,
					amount,
					deposit_details: *extrinsic_index,
				});
			} else {
				tracing::warn!(
					"Detected transfer to monitored address {:?} of asset {:?} with amount {} in block {:?}, but no liquid balance increase was available to witness after accounting for outgoing transfers and previous events in the same block. This transfer will not be witnessed.",
					deposit_address, asset, amount, block_hash
				);
			}
		}
	}
	deposit_witnesses
}

#[cfg(test)]
mod test {
	use cf_chains::dot::PolkadotBalance;
	use cf_primitives::ASSETHUB_USDC_ASSET_ID;

	use crate::{
		dot::retry_rpc::mocks::MockDotRpcClient,
		witness::hub::{test::phase_and_events, HubAssetId},
	};

	use super::*;

	fn mock_transfer(
		from: &PolkadotAccountId,
		to: &PolkadotAccountId,
		amount: PolkadotBalance,
	) -> EventWrapper {
		EventWrapper::BalancesTransfer {
			from: from.aliased_ref().to_owned().into(),
			to: to.aliased_ref().to_owned().into(),
			amount,
		}
	}

	fn mock_assets_transfer(
		asset_id: HubAssetId,
		from: &PolkadotAccountId,
		to: &PolkadotAccountId,
		amount: PolkadotBalance,
	) -> EventWrapper {
		EventWrapper::AssetsTransfer {
			asset_id,
			from: from.aliased_ref().to_owned().into(),
			to: to.aliased_ref().to_owned().into(),
			amount,
		}
	}

	/// Returns a mock client whose `liquid_account_balance` reports a huge unlocked
	/// balance at the current block and zero at the parent — i.e. every transfer is
	/// fully liquid, so witness amounts are not clamped.
	fn mock_client_all_liquid(block_hash: PolkadotHash) -> MockDotRpcClient {
		let mut client = MockDotRpcClient::new();
		client.expect_liquid_account_balance().returning(move |_, _, hash| {
			if hash == block_hash {
				u128::MAX / 2
			} else {
				0
			}
		});
		client
	}

	#[tokio::test]
	async fn witness_deposits_for_addresses_we_monitor() {
		let our_vault = PolkadotAccountId::from_aliased([0; 32]);

		// We want two monitors, one sent through at start, and one sent through channel
		const TRANSFER_1_INDEX: u32 = 1;
		let transfer_1_deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const TRANSFER_1_AMOUNT: PolkadotBalance = 10000;

		const TRANSFER_2_INDEX: u32 = 2;
		let transfer_2_deposit_address = PolkadotAccountId::from_aliased([2; 32]);
		const TRANSFER_2_AMOUNT: PolkadotBalance = 20000;

		const TRANSFER_3_INDEX: u32 = 3;
		let transfer_3_deposit_address = PolkadotAccountId::from_aliased([2; 32]);
		const TRANSFER_3_AMOUNT: PolkadotBalance = 30000;

		const TRANSFER_4_INDEX: u32 = 3;
		let transfer_4_deposit_address = PolkadotAccountId::from_aliased([2; 32]);
		const TRANSFER_4_AMOUNT: PolkadotBalance = 40000;

		const TRANSFER_FROM_OUR_VAULT_INDEX: u32 = 7;
		const TRANSFER_TO_OUR_VAULT_INDEX: u32 = 8;

		const TRANSFER_TO_SELF_INDEX: u32 = 9;
		const TRANSFER_TO_SELF_AMOUNT: PolkadotBalance = 30000;

		let block_event_details = phase_and_events(vec![
			// we'll be witnessing this from the start
			(
				TRANSFER_1_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&transfer_1_deposit_address,
					TRANSFER_1_AMOUNT,
				),
			),
			// we'll receive this address from the channel
			(
				TRANSFER_2_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&transfer_2_deposit_address,
					TRANSFER_2_AMOUNT,
				),
			),
			// an assethub USDC `assets` transfer
			(
				TRANSFER_3_INDEX,
				mock_assets_transfer(
					ASSETHUB_USDC_ASSET_ID,
					&PolkadotAccountId::from_aliased([7; 32]),
					&transfer_3_deposit_address,
					TRANSFER_3_AMOUNT,
				),
			),
			// an assethub USDT `assets` transfer
			(
				TRANSFER_3_INDEX,
				mock_assets_transfer(
					ASSETHUB_USDT_ASSET_ID,
					&PolkadotAccountId::from_aliased([7; 32]),
					&transfer_4_deposit_address,
					TRANSFER_4_AMOUNT,
				),
			),
			// this one is not for us
			(
				19,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&PolkadotAccountId::from_aliased([9; 32]),
					93232,
				),
			),
			(
				TRANSFER_FROM_OUR_VAULT_INDEX,
				mock_transfer(&our_vault, &PolkadotAccountId::from_aliased([9; 32]), 93232),
			),
			(
				TRANSFER_TO_OUR_VAULT_INDEX,
				mock_transfer(&PolkadotAccountId::from_aliased([9; 32]), &our_vault, 93232),
			),
			// Example: Someone generates a DOT -> ETH swap, getting the DOT address that we're now
			// monitoring for inputs. They now generate a BTC -> DOT swap, and set the destination
			// address of the DOT to the address they generated earlier.
			// Now our Polkadot vault is sending to an address we're monitoring for deposits.
			(
				TRANSFER_TO_SELF_INDEX,
				mock_transfer(&our_vault, &transfer_2_deposit_address, TRANSFER_TO_SELF_AMOUNT),
			),
		]);

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);
		let hub_client = mock_client_all_liquid(block_hash);

		let deposit_witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![transfer_1_deposit_address, transfer_2_deposit_address],
			&block_event_details,
		)
		.await;

		assert_eq!(
			deposit_witnesses,
			vec![
				DepositWitness {
					deposit_address: transfer_1_deposit_address,
					asset: HubAsset::HubDot,
					amount: TRANSFER_1_AMOUNT,
					deposit_details: TRANSFER_1_INDEX
				},
				DepositWitness {
					deposit_address: transfer_2_deposit_address,
					asset: HubAsset::HubDot,
					amount: TRANSFER_2_AMOUNT,
					deposit_details: TRANSFER_2_INDEX
				},
				DepositWitness {
					deposit_address: transfer_3_deposit_address,
					asset: HubAsset::HubUsdc,
					amount: TRANSFER_3_AMOUNT,
					deposit_details: TRANSFER_3_INDEX
				},
				DepositWitness {
					deposit_address: transfer_4_deposit_address,
					asset: HubAsset::HubUsdt,
					amount: TRANSFER_4_AMOUNT,
					deposit_details: TRANSFER_4_INDEX
				},
				DepositWitness {
					deposit_address: transfer_2_deposit_address,
					asset: HubAsset::HubDot,
					amount: TRANSFER_TO_SELF_AMOUNT,
					deposit_details: TRANSFER_TO_SELF_INDEX
				}
			]
		);
	}

	/// A vested transfer increases the recipient's `free` and `frozen` balance by the same
	/// amount, so the *liquid* balance increase is zero. The witness must therefore credit
	/// zero, not the raw transfer amount.
	#[tokio::test]
	async fn vested_transfer_credits_only_liquid_increase() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const TRANSFER_INDEX: u32 = 1;
		const TRANSFER_AMOUNT: PolkadotBalance = 40_000_000_000; // 4 DOT, as in the report

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);

		// Liquid balance is zero at both blocks because the incoming funds are
		// fully locked by a vesting schedule.
		let mut hub_client = MockDotRpcClient::new();
		hub_client.expect_liquid_account_balance().returning(|_, _, _| 0);

		let events = phase_and_events(vec![(
			TRANSFER_INDEX,
			mock_transfer(
				&PolkadotAccountId::from_aliased([7; 32]),
				&deposit_address,
				TRANSFER_AMOUNT,
			),
		)]);

		let witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![deposit_address],
			&events,
		)
		.await;

		// The clamped deposit is zero, so no witness is emitted (a warning is logged).
		assert!(witnesses.is_empty());
	}

	/// Builds a mock that reports the recipient's liquid balance as `parent_liquid` at the
	/// parent block and `current_liquid` at the current block (any other hash → 0).
	fn mock_client_with_balances(
		block_hash: PolkadotHash,
		parent_hash: PolkadotHash,
		current_liquid: u128,
		parent_liquid: u128,
	) -> MockDotRpcClient {
		let mut client = MockDotRpcClient::new();
		client.expect_liquid_account_balance().returning(move |_, _, hash| {
			if hash == block_hash {
				current_liquid
			} else if hash == parent_hash {
				parent_liquid
			} else {
				0
			}
		});
		client
	}

	/// Two regular transfers to the same address in the same block. The recipient's
	/// liquid balance increases by the full sum, so both witnesses are credited in full.
	#[tokio::test]
	async fn two_regular_transfers_same_address_both_credited() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const TRANSFER_A_INDEX: u32 = 1;
		const TRANSFER_A_AMOUNT: PolkadotBalance = 10_000;
		const TRANSFER_B_INDEX: u32 = 2;
		const TRANSFER_B_AMOUNT: PolkadotBalance = 25_000;

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);

		// Parent block: 0 liquid. Current block: full sum of both transfers liquid.
		let hub_client = mock_client_with_balances(
			block_hash,
			parent_hash,
			TRANSFER_A_AMOUNT + TRANSFER_B_AMOUNT,
			0,
		);

		let events = phase_and_events(vec![
			(
				TRANSFER_A_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&deposit_address,
					TRANSFER_A_AMOUNT,
				),
			),
			(
				TRANSFER_B_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([8; 32]),
					&deposit_address,
					TRANSFER_B_AMOUNT,
				),
			),
		]);

		let witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![deposit_address],
			&events,
		)
		.await;

		assert_eq!(
			witnesses,
			vec![
				DepositWitness {
					deposit_address,
					asset: HubAsset::HubDot,
					amount: TRANSFER_A_AMOUNT,
					deposit_details: TRANSFER_A_INDEX,
				},
				DepositWitness {
					deposit_address,
					asset: HubAsset::HubDot,
					amount: TRANSFER_B_AMOUNT,
					deposit_details: TRANSFER_B_INDEX,
				},
			]
		);
	}

	/// A regular transfer followed by a vested transfer in the same block. The liquid
	/// balance only increases by the regular amount; the total credited must match.
	#[tokio::test]
	async fn regular_then_vested_transfer_same_address() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const REGULAR_INDEX: u32 = 1;
		const REGULAR_AMOUNT: PolkadotBalance = 10_000;
		const VESTED_INDEX: u32 = 2;
		const VESTED_AMOUNT: PolkadotBalance = 40_000_000_000;

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);

		// Liquid balance rises by exactly the regular transfer amount; the vested
		// transfer raises `free` and `frozen` by the same amount, contributing 0.
		let hub_client = mock_client_with_balances(block_hash, parent_hash, REGULAR_AMOUNT, 0);

		let events = phase_and_events(vec![
			(
				REGULAR_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&deposit_address,
					REGULAR_AMOUNT,
				),
			),
			(
				VESTED_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([8; 32]),
					&deposit_address,
					VESTED_AMOUNT,
				),
			),
		]);

		let witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![deposit_address],
			&events,
		)
		.await;

		// The first (regular) event exhausts the available liquid budget. The second
		// (vested) event clamps to zero and is suppressed.
		assert_eq!(witnesses.len(), 1);
		assert_eq!(witnesses[0].amount, REGULAR_AMOUNT);
		assert_eq!(witnesses[0].deposit_details, REGULAR_INDEX);
	}

	/// Vested transfer first, then a regular transfer. Ordering changes the distribution
	/// across the two witnesses, but the total credited must still equal the liquid
	/// increase (i.e. the regular amount only).
	#[tokio::test]
	async fn vested_then_regular_transfer_same_address() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const VESTED_INDEX: u32 = 1;
		const VESTED_AMOUNT: PolkadotBalance = 40_000_000_000;
		const REGULAR_INDEX: u32 = 2;
		const REGULAR_AMOUNT: PolkadotBalance = 10_000;

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);

		let hub_client = mock_client_with_balances(block_hash, parent_hash, REGULAR_AMOUNT, 0);

		let events = phase_and_events(vec![
			(
				VESTED_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&deposit_address,
					VESTED_AMOUNT,
				),
			),
			(
				REGULAR_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([8; 32]),
					&deposit_address,
					REGULAR_AMOUNT,
				),
			),
		]);

		let witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![deposit_address],
			&events,
		)
		.await;

		// FIFO: the vested event arrives first and absorbs the entire liquid budget
		// (REGULAR_AMOUNT). The regular event that follows clamps to zero and is
		// suppressed. The aggregate credited still equals the real liquid increase.
		assert_eq!(witnesses.len(), 1);
		assert_eq!(witnesses[0].amount, REGULAR_AMOUNT);
		assert_eq!(witnesses[0].deposit_details, VESTED_INDEX);
	}

	/// Pre-existing liquid balance at the parent block must not be counted towards the
	/// deposit — only the delta (current - parent) is credited.
	#[tokio::test]
	async fn pre_existing_balance_is_not_credited() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const TRANSFER_INDEX: u32 = 1;
		const TRANSFER_AMOUNT: PolkadotBalance = 1_000;
		const PRE_EXISTING: PolkadotBalance = 9_000;

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);

		// The account already held PRE_EXISTING liquid funds before the block. After
		// the transfer it holds PRE_EXISTING + TRANSFER_AMOUNT. Only the delta should
		// be credited.
		let hub_client = mock_client_with_balances(
			block_hash,
			parent_hash,
			PRE_EXISTING + TRANSFER_AMOUNT,
			PRE_EXISTING,
		);

		let events = phase_and_events(vec![(
			TRANSFER_INDEX,
			mock_transfer(
				&PolkadotAccountId::from_aliased([7; 32]),
				&deposit_address,
				TRANSFER_AMOUNT,
			),
		)]);

		let witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![deposit_address],
			&events,
		)
		.await;

		assert_eq!(witnesses.len(), 1);
		assert_eq!(witnesses[0].amount, TRANSFER_AMOUNT);
	}

	/// At the genesis block there is no parent. The previous-balance fetch must be
	/// skipped (not silently queried with some bogus hash), and the full liquid
	/// balance at the current block is credited.
	#[tokio::test]
	async fn genesis_block_has_no_parent_hash() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const TRANSFER_INDEX: u32 = 1;
		const TRANSFER_AMOUNT: PolkadotBalance = 5_000;

		let block_hash = PolkadotHash::from([1u8; 32]);

		// The mock panics if it is called with any hash other than `block_hash`,
		// which would mean the code tried to fetch a previous-block balance even
		// though no parent was supplied.
		let mut hub_client = MockDotRpcClient::new();
		hub_client
			.expect_liquid_account_balance()
			.withf(move |_, _, hash| *hash == block_hash)
			.returning(move |_, _, _| TRANSFER_AMOUNT);

		let events = phase_and_events(vec![(
			TRANSFER_INDEX,
			mock_transfer(
				&PolkadotAccountId::from_aliased([7; 32]),
				&deposit_address,
				TRANSFER_AMOUNT,
			),
		)]);

		let witnesses =
			deposit_witnesses(block_hash, None, &hub_client, vec![deposit_address], &events).await;

		assert_eq!(witnesses.len(), 1);
		assert_eq!(witnesses[0].amount, TRANSFER_AMOUNT);
	}

	/// Two transfers to the same address in the same block, but in different assets
	/// (HubDot and HubUsdc). The liquid-balance budget is per `(address, asset)`,
	/// so the two transfers must not share a budget and both must be credited in full.
	#[tokio::test]
	async fn same_address_different_assets_tracked_independently() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const DOT_INDEX: u32 = 1;
		const DOT_AMOUNT: PolkadotBalance = 1_000;
		const USDC_INDEX: u32 = 2;
		const USDC_AMOUNT: PolkadotBalance = 2_000;

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);

		// Each asset has exactly its transfer amount as liquid increase at the
		// current block, and zero at the parent.
		let mut hub_client = MockDotRpcClient::new();
		hub_client.expect_liquid_account_balance().returning(move |_, asset, hash| {
			if hash == block_hash {
				match asset {
					HubAsset::HubDot => DOT_AMOUNT,
					HubAsset::HubUsdc => USDC_AMOUNT,
					HubAsset::HubUsdt => 0,
				}
			} else {
				0
			}
		});

		let events = phase_and_events(vec![
			(
				DOT_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&deposit_address,
					DOT_AMOUNT,
				),
			),
			(
				USDC_INDEX,
				mock_assets_transfer(
					ASSETHUB_USDC_ASSET_ID,
					&PolkadotAccountId::from_aliased([7; 32]),
					&deposit_address,
					USDC_AMOUNT,
				),
			),
		]);

		let witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![deposit_address],
			&events,
		)
		.await;

		assert_eq!(
			witnesses,
			vec![
				DepositWitness {
					deposit_address,
					asset: HubAsset::HubDot,
					amount: DOT_AMOUNT,
					deposit_details: DOT_INDEX,
				},
				DepositWitness {
					deposit_address,
					asset: HubAsset::HubUsdc,
					amount: USDC_AMOUNT,
					deposit_details: USDC_INDEX,
				},
			]
		);
	}

	/// A vault sweep removes prior liquid funds in the same block as a new deposit
	/// arrives. The liquid delta `current - parent` understates the incoming amount,
	/// so the outgoing transfer must be added back to the budget for the deposit to
	/// be fully credited.
	#[tokio::test]
	async fn outgoing_then_incoming_same_block_credits_in_full() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		let vault = PolkadotAccountId::from_aliased([7; 32]);
		const SWEEP_INDEX: u32 = 1;
		const SWEEP_AMOUNT: PolkadotBalance = 50;
		const DEPOSIT_INDEX: u32 = 2;
		const DEPOSIT_AMOUNT: PolkadotBalance = 100;

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);

		// Parent held SWEEP_AMOUNT liquid; after the sweep + deposit the account holds
		// DEPOSIT_AMOUNT. The naive delta is DEPOSIT_AMOUNT - SWEEP_AMOUNT; the outgoing
		// transfer is added back so the full deposit is credited.
		let hub_client =
			mock_client_with_balances(block_hash, parent_hash, DEPOSIT_AMOUNT, SWEEP_AMOUNT);

		let events = phase_and_events(vec![
			(SWEEP_INDEX, mock_transfer(&deposit_address, &vault, SWEEP_AMOUNT)),
			(DEPOSIT_INDEX, mock_transfer(&vault, &deposit_address, DEPOSIT_AMOUNT)),
		]);

		let witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![deposit_address],
			&events,
		)
		.await;

		assert_eq!(witnesses.len(), 1);
		assert_eq!(witnesses[0].amount, DEPOSIT_AMOUNT);
	}

	/// A new deposit arrives, then a vault sweep takes both the prior balance and
	/// the new deposit out of the account in the same block. The current liquid
	/// balance drops below the parent — so `saturating_sub` clamps the delta to zero
	/// — but the deposit is real and must be credited in full once the outgoing
	/// transfers are added back to the budget.
	#[tokio::test]
	async fn incoming_then_outgoing_same_block_with_balance_drop_credits_in_full() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		let vault = PolkadotAccountId::from_aliased([7; 32]);
		const PARENT_LIQUID: PolkadotBalance = 50;
		const DEPOSIT_INDEX: u32 = 1;
		const DEPOSIT_AMOUNT: PolkadotBalance = 100;
		const SWEEP_INDEX: u32 = 2;
		const SWEEP_AMOUNT: PolkadotBalance = PARENT_LIQUID + DEPOSIT_AMOUNT;

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);

		// Parent: PARENT_LIQUID. After deposit + sweep-of-everything, current = 0.
		// Without the outgoing-sum compensation the deposit would silently witness
		// as zero, because `saturating_sub(0, 50)` is 0.
		let hub_client = mock_client_with_balances(block_hash, parent_hash, 0, PARENT_LIQUID);

		let events = phase_and_events(vec![
			(DEPOSIT_INDEX, mock_transfer(&vault, &deposit_address, DEPOSIT_AMOUNT)),
			(SWEEP_INDEX, mock_transfer(&deposit_address, &vault, SWEEP_AMOUNT)),
		]);

		let witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![deposit_address],
			&events,
		)
		.await;

		assert_eq!(witnesses.len(), 1);
		assert_eq!(witnesses[0].amount, DEPOSIT_AMOUNT);
	}

	/// An outgoing transfer in one asset must not inflate the budget for a different
	/// asset on the same address. The budget map is keyed by `(address, asset)` on
	/// both the incoming and outgoing sides.
	#[tokio::test]
	async fn outgoing_in_one_asset_does_not_inflate_other_asset_budget() {
		let deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		let vault = PolkadotAccountId::from_aliased([7; 32]);
		const DOT_SWEEP_INDEX: u32 = 1;
		const DOT_SWEEP_AMOUNT: PolkadotBalance = 1_000;
		// A vested USDC-like deposit: the event amount is large, but the liquid
		// delta is zero. If the DOT outgoing were leaking into the USDC budget,
		// this would be credited as DOT_SWEEP_AMOUNT instead of zero.
		const USDC_DEPOSIT_INDEX: u32 = 2;
		const USDC_DEPOSIT_AMOUNT: PolkadotBalance = 5_000;

		let block_hash = PolkadotHash::from([1u8; 32]);
		let parent_hash = PolkadotHash::from([0u8; 32]);

		// Parent: DOT_SWEEP_AMOUNT in HubDot, zero in USDC. Current: zero in both
		// (DOT swept out, USDC arrived but is fully locked → zero liquid).
		let mut hub_client = MockDotRpcClient::new();
		hub_client.expect_liquid_account_balance().returning(move |_, asset, hash| {
			match (asset, hash) {
				(HubAsset::HubDot, h) if h == parent_hash => DOT_SWEEP_AMOUNT,
				_ => 0,
			}
		});

		let events = phase_and_events(vec![
			(DOT_SWEEP_INDEX, mock_transfer(&deposit_address, &vault, DOT_SWEEP_AMOUNT)),
			(
				USDC_DEPOSIT_INDEX,
				mock_assets_transfer(
					ASSETHUB_USDC_ASSET_ID,
					&vault,
					&deposit_address,
					USDC_DEPOSIT_AMOUNT,
				),
			),
		]);

		let witnesses = deposit_witnesses(
			block_hash,
			Some(parent_hash),
			&hub_client,
			vec![deposit_address],
			&events,
		)
		.await;

		// The USDC event clamps to zero (its liquid delta is zero and the DOT outgoing
		// must not bleed across asset budgets), so no witness is emitted.
		assert!(witnesses.is_empty());
	}
}
