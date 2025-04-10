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

use super::*;
use cf_chains::{
	btc::{deposit_address::DepositAddress, ScriptPubkey, Utxo},
	eth::api::EthereumApi,
	AllBatch, ApiCall, Bitcoin, ForeignChain, TransferAssetParams, UpdateFlipSupply,
};
use cf_primitives::{chains::assets::btc, AuthorityCount, BroadcastId};
use cf_traits::{Broadcaster, EpochInfo};
use pallet_cf_broadcast::{AwaitingBroadcast, DelayedBroadcastRetryQueue, PendingBroadcasts};
use state_chain_runtime::{
	BitcoinBroadcaster, BitcoinInstance, BitcoinThresholdSigner, Environment, Runtime, Validator,
};

#[test]
fn bitcoin_broadcast_delay_works() {
	const EPOCH_DURATION_BLOCKS: u32 = 200;
	const MAX_AUTHORITIES: AuthorityCount = 150;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			// Create a network of 150 validators
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			testnet.move_to_the_next_epoch();
			assert_eq!(Validator::current_authorities().len(), 150);
			let epoch = Validator::epoch_index();
			let bitcoin_agg_key = BitcoinThresholdSigner::keys(epoch).unwrap().current;
			let egress_id = (ForeignChain::Bitcoin, 1u64);
			Environment::add_bitcoin_utxo_to_list(Utxo {
				id: Default::default(),
				amount: 1_000_000_000_000u64,
				deposit_address: DepositAddress::new(bitcoin_agg_key, 0u32),
			});

			// Cause bitcoin vault to rotate - but stop the broadcasting.
			let (bitcoin_call, _egress_ids) = AllBatch::<Bitcoin>::new_unsigned(
				vec![],
				vec![(
					TransferAssetParams::<Bitcoin> {
						asset: btc::Asset::Btc,
						amount: 1_000_000,
						to: ScriptPubkey::P2PKH([0u8; 20]),
					},
					egress_id,
				)],
			)
			.unwrap()[0]
				.clone();

			let (broadcast_id, _) =
				<BitcoinBroadcaster as Broadcaster<Bitcoin>>::threshold_sign_and_broadcast(
					bitcoin_call,
				);
			assert!(PendingBroadcasts::<Runtime, BitcoinInstance>::get().contains(&broadcast_id));
			// Finish threshold signing.
			testnet.move_forward_blocks(11);
			assert!(AwaitingBroadcast::<Runtime, BitcoinInstance>::contains_key(broadcast_id));

			let delay_sequence = [
				1u32, 2u32, 4u32, 8u32, 16u32, 32u32, 64u32, 128u32, 256u32, 512u32, 1024u32,
				1200u32,
			];
			// Same as defined in BitcoinRetryPolicy.
			const DELAY_THRESHOLD: u32 = 25u32;

			let get_nominee = |broadcast_id: BroadcastId| {
				AwaitingBroadcast::<Runtime, BitcoinInstance>::get(broadcast_id)
					.unwrap()
					.nominee
					.unwrap()
			};

			// Before hitting the threshold, no slowdown happens and broadcasts are retried per
			// normal.
			for _ in 1u32..DELAY_THRESHOLD {
				let account_id = get_nominee(broadcast_id);
				assert_ok!(BitcoinBroadcaster::transaction_failed(
					RuntimeOrigin::signed(account_id),
					broadcast_id
				));
				let next_block = System::block_number() + 1u32;
				assert!(DelayedBroadcastRetryQueue::<Runtime, BitcoinInstance>::get(next_block)
					.contains(&broadcast_id));
				testnet.move_forward_blocks(1);
				assert!(AwaitingBroadcast::<Runtime, BitcoinInstance>::contains_key(broadcast_id));
			}

			// Following failed broadcasts are delayed by a increasing sequence.
			// Delay caps at 1200.
			testnet.set_active_all_nodes(false);
			for delay in delay_sequence {
				let account_id = get_nominee(broadcast_id);
				let target_retry_block = System::block_number() + delay;

				assert_ok!(BitcoinBroadcaster::transaction_failed(
					RuntimeOrigin::signed(account_id),
					broadcast_id
				));

				assert!(DelayedBroadcastRetryQueue::<Runtime, BitcoinInstance>::get(
					target_retry_block
				)
				.contains(&broadcast_id));

				testnet.move_forward_blocks(delay);

				assert!(AwaitingBroadcast::<Runtime, BitcoinInstance>::contains_key(broadcast_id));
				assert_eq!(
					DelayedBroadcastRetryQueue::<Runtime, BitcoinInstance>::decode_non_dedup_len(
						System::block_number()
					),
					None
				);
			}
		});
}

#[test]
fn refresh_replay_protection() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let mut api_call = <<Runtime as pallet_cf_emissions::Config>::ApiCall as UpdateFlipSupply<_>>::new_unsigned(1_000_000, 1);

		let old_replay_protection = match &api_call {
			EthereumApi::UpdateFlipSupply(call) => call.replay_protection(),
			_ => unreachable!("Expected EthereumApi::UpdateFlipSupply"),
		};

		api_call.refresh_replay_protection();
		let new_replay_protection = match &api_call {
			EthereumApi::UpdateFlipSupply(call) => call.replay_protection(),
			_ => unreachable!("Expected EthereumApi::UpdateFlipSupply"),
		};

		assert_ne!(old_replay_protection, new_replay_protection);
		// Only the nonce should change
		assert_ne!(old_replay_protection.nonce, new_replay_protection.nonce);
		assert_eq!(old_replay_protection.chain_id, new_replay_protection.chain_id);
		assert_eq!(old_replay_protection.contract_address, new_replay_protection.contract_address);
	});
}
