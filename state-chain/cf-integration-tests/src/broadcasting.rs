use super::*;
use cf_chains::{
	btc::{deposit_address::DepositAddress, BitcoinRetryPolicy, ScriptPubkey},
	AllBatch, Bitcoin, RetryPolicy, TransferAssetParams,
};
use cf_primitives::{chains::assets::btc, AuthorityCount};
use cf_traits::{Broadcaster, EpochInfo};
use pallet_cf_broadcast::{
	AwaitingBroadcast, BroadcastRetryQueue, DelayedBroadcastRetryQueue, PendingBroadcasts,
};
use state_chain_runtime::{
	BitcoinBroadcaster, BitcoinInstance, BitcoinVault, Environment, Runtime, Validator,
};

#[test]
fn bitcoin_broadcast_delay_works() {
	const EPOCH_BLOCKS: u32 = 200;
	const MAX_AUTHORITIES: AuthorityCount = 150;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			// Create a network of 150 validators
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			testnet.move_to_the_next_epoch();
			assert_eq!(Validator::current_authorities().len(), 150);
			let epoch = Validator::epoch_index();
			let bitcoin_agg_key = BitcoinVault::vaults(epoch).unwrap().public_key.current;
			Environment::add_bitcoin_utxo_to_list(
				1_000_000_000_000u64,
				Default::default(),
				DepositAddress::new(bitcoin_agg_key, 0u32),
			);

			// Cause bitcoin vault to rotate - but stop the broadcasting.
			let bitcoin_call = AllBatch::<Bitcoin>::new_unsigned(
				vec![],
				vec![TransferAssetParams::<Bitcoin> {
					asset: btc::Asset::Btc,
					amount: 1_000_000,
					to: ScriptPubkey::P2PKH([0u8; 20]),
				}],
			)
			.unwrap();

			let broadcast_id =
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
			// Before hitting the threshold, no slowdown happens and broadcasts are retried per
			// normal.
			for i in 1u32..DELAY_THRESHOLD {
				let account_id = AccountId::from([i as u8; 32]);
				assert_ok!(BitcoinBroadcaster::transaction_failed(
					RuntimeOrigin::signed(account_id),
					broadcast_id
				));
				assert_eq!(
					BroadcastRetryQueue::<Runtime, BitcoinInstance>::get()[0].broadcast_id,
					broadcast_id
				);
				testnet.move_forward_blocks(1);
				assert!(AwaitingBroadcast::<Runtime, BitcoinInstance>::contains_key(broadcast_id));
			}

			// Following failed broadcasts are delayed by a increasing sequence.
			// Delay caps at 1200.
			for (i, delay) in delay_sequence.into_iter().enumerate() {
				let account_id = AccountId::from([(DELAY_THRESHOLD + i as u32) as u8; 32]);
				let target_retry_block = System::block_number() + delay;
				assert_ok!(BitcoinBroadcaster::transaction_failed(
					RuntimeOrigin::signed(account_id),
					broadcast_id
				));
				assert_eq!(BroadcastRetryQueue::<Runtime, BitcoinInstance>::decode_len(), Some(0));
				if DelayedBroadcastRetryQueue::<Runtime, BitcoinInstance>::get(target_retry_block)
					.is_empty()
				{
					println!("{}, {}", System::block_number(), target_retry_block);
				}
				testnet.move_forward_blocks(delay);
				assert!(AwaitingBroadcast::<Runtime, BitcoinInstance>::contains_key(broadcast_id));
				assert_eq!(
					DelayedBroadcastRetryQueue::<Runtime, BitcoinInstance>::decode_len(
						System::block_number()
					),
					None
				);
			}
		});
}
