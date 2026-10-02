// Copyright 2026 Chainflip Labs GmbH
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
	ChannelCleanupRetryBlocks, DepositChannelRecycleBlocks, RejectionDelayBlocks,
	ScheduledTransactionsForRejection, DEFAULT_REJECTION_DELAY_BLOCKS,
};
use cf_chains::{evm::DeploymentStatus, Bitcoin};
use cf_traits::IngressSink;
use frame_support::{instances::Instance3, traits::BuildGenesisConfig};

fn empty_channel() -> (ChannelId, EthereumAddress) {
	let (channel_id, address, _, _) = EthereumIngressEgress::request_liquidity_deposit_address(
		BROKER,
		BROKER,
		EthAsset::Eth,
		0,
		ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
		None,
	)
	.unwrap();
	(channel_id, address.try_into().unwrap())
}

fn rejected_channel() -> (ChannelId, EthereumAddress) {
	let (channel_id, address) = empty_channel();
	assert_ok!(EthereumIngressEgress::mark_deposit_channel_for_rejection(
		RuntimeOrigin::signed(BROKER),
		address,
	));
	witness_rejected_deposit(address);
	(channel_id, address)
}

fn witness_rejected_deposit(address: EthereumAddress) {
	EthereumIngressEgress::process_channel_deposit_full_witness(
		DepositWitness {
			deposit_address: address,
			asset: EthAsset::Eth,
			amount: 1_000_000,
			deposit_details: Default::default(),
		},
		1,
	);
}

#[test]
fn rejection_delay_is_set_at_genesis_and_defaults_to_one_day() {
	new_test_ext().execute_with(|| {
		// The mock genesis sets a zero delay.
		assert_eq!(RejectionDelayBlocks::<Test, Instance1>::get(), 0);

		crate::GenesisConfig::<Test, Instance1> { rejection_delay_blocks: 7, ..Default::default() }
			.build();
		assert_eq!(RejectionDelayBlocks::<Test, Instance1>::get(), 7);

		crate::GenesisConfig::<Test, Instance1>::default().build();
		assert_eq!(
			RejectionDelayBlocks::<Test, Instance1>::get(),
			u64::from(DEFAULT_REJECTION_DELAY_BLOCKS)
		);
	});
}

#[test]
fn rejection_delay_defaults_to_one_day_and_is_governance_configurable() {
	new_test_ext().execute_with(|| {
		RejectionDelayBlocks::<Test, Instance1>::kill();
		assert_eq!(
			RejectionDelayBlocks::<Test, Instance1>::get(),
			u64::from(DEFAULT_REJECTION_DELAY_BLOCKS)
		);
		assert_noop!(
			EthereumIngressEgress::update_pallet_config(
				RuntimeOrigin::signed(BROKER),
				vec![PalletConfigUpdate::SetRejectionDelay { delay_blocks: 4 }]
					.try_into()
					.unwrap(),
			),
			sp_runtime::traits::BadOrigin
		);
		assert_ok!(EthereumIngressEgress::update_pallet_config(
			RuntimeOrigin::root(),
			vec![PalletConfigUpdate::SetRejectionDelay { delay_blocks: 4 }]
				.try_into()
				.unwrap(),
		));
		assert_eq!(RejectionDelayBlocks::<Test, Instance1>::get(), 4);
	});
}

#[test]
fn channel_cleanup_retry_has_chain_specific_defaults_and_is_configurable_per_instance() {
	new_test_ext().execute_with(|| {
		assert_eq!(ChannelCleanupRetryBlocks::<Test, Instance1>::get(), 50);
		assert_eq!(ChannelCleanupRetryBlocks::<Test, Instance2>::get(), 1);
		let update = PalletConfigUpdate::SetChannelCleanupRetry { retry_blocks: 7 };
		assert_noop!(
			EthereumIngressEgress::update_pallet_config(
				RuntimeOrigin::signed(BROKER),
				vec![update.clone()].try_into().unwrap(),
			),
			sp_runtime::traits::BadOrigin
		);
		assert_ok!(EthereumIngressEgress::update_pallet_config(
			RuntimeOrigin::root(),
			vec![update.clone()].try_into().unwrap(),
		));
		assert_eq!(ChannelCleanupRetryBlocks::<Test, Instance1>::get(), 7);
		assert_eq!(ChannelCleanupRetryBlocks::<Test, Instance2>::get(), 1);
		System::assert_last_event(RuntimeEvent::EthereumIngressEgress(
			Event::PalletConfigUpdated { update },
		));

		assert_ok!(BitcoinIngressEgress::update_pallet_config(
			RuntimeOrigin::root(),
			vec![PalletConfigUpdate::SetChannelCleanupRetry { retry_blocks: 12 }]
				.try_into()
				.unwrap(),
		));
		assert_eq!(ChannelCleanupRetryBlocks::<Test, Instance1>::get(), 7);
		assert_eq!(ChannelCleanupRetryBlocks::<Test, Instance2>::get(), 12);

		assert_ok!(EthereumIngressEgress::update_pallet_config(
			RuntimeOrigin::root(),
			vec![PalletConfigUpdate::SetChannelCleanupRetry { retry_blocks: 0 }]
				.try_into()
				.unwrap(),
		));
		assert_eq!(ChannelCleanupRetryBlocks::<Test, Instance1>::get(), 0);
		assert_eq!(ChannelCleanupRetryBlocks::<Test, Instance2>::get(), 12);
	});
}

#[test]
fn delayed_refund_retains_channel_until_deployment_completes() {
	new_test_ext().execute_with(|| {
		System::set_block_number(10);
		RejectionDelayBlocks::<Test, Instance1>::set(4);
		ChannelCleanupRetryBlocks::<Test, Instance1>::set(100);
		let (channel_id, address) = rejected_channel();
		let channel = DepositChannelLookup::<Test, Instance1>::get(address).unwrap();
		assert_eq!(ScheduledTransactionsForRejection::<Test, Instance1>::get(14).len(), 1);

		// A configuration change must not shorten an already scheduled hold.
		RejectionDelayBlocks::<Test, Instance1>::set(0);
		EthereumIngressEgress::on_finalize(13);
		assert!(MockEgressBroadcasterEth::get_pending_api_calls().is_empty());
		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());

		let recycle_at = DepositChannelRecycleBlocks::<Test, Instance1>::get()[0].0;
		set_eth_processed_up_to(recycle_at);
		EthereumIngressEgress::on_idle(13, Weight::MAX);
		assert_eq!(DepositChannelLookup::<Test, Instance1>::get(address), Some(channel));
		assert!(!DepositChannelPool::<Test, Instance1>::contains_key(channel_id));
		assert_eq!(DepositChannelRecycleBlocks::<Test, Instance1>::get().len(), 1);

		EthereumIngressEgress::on_finalize(14);
		assert!(ScheduledTransactionsForRejection::<Test, Instance1>::iter().next().is_none());
		assert_eq!(MockEgressBroadcasterEth::get_pending_api_calls().len(), 1);
		let retry_at = DepositChannelRecycleBlocks::<Test, Instance1>::get()[0].0;
		assert_eq!(retry_at, recycle_at + 100);
		set_eth_processed_up_to(retry_at);
		EthereumIngressEgress::on_idle(15, Weight::MAX);
		assert_eq!(
			DepositChannelLookup::<Test, Instance1>::get(address)
				.unwrap()
				.deposit_channel
				.state,
			DeploymentStatus::Pending
		);
		assert!(!DepositChannelPool::<Test, Instance1>::contains_key(channel_id));
		assert_eq!(DepositChannelRecycleBlocks::<Test, Instance1>::get().len(), 1);

		let broadcast_id = BroadcastActions::<Test, Instance1>::iter_keys().next().unwrap();
		EthereumIngressEgress::on_broadcast_success(broadcast_id, retry_at);
		let retry_at = DepositChannelRecycleBlocks::<Test, Instance1>::get()[0].0;
		set_eth_processed_up_to(retry_at - 1);
		EthereumIngressEgress::on_idle(16, Weight::MAX);
		assert!(DepositChannelLookup::<Test, Instance1>::contains_key(address));
		assert!(!DepositChannelPool::<Test, Instance1>::contains_key(channel_id));
		set_eth_processed_up_to(retry_at);
		EthereumIngressEgress::on_idle(16, Weight::MAX);
		assert!(!DepositChannelLookup::<Test, Instance1>::contains_key(address));
		assert!(DepositChannelPool::<Test, Instance1>::contains_key(channel_id));
		assert!(DepositChannelRecycleBlocks::<Test, Instance1>::get().is_empty());
	});
}

#[test]
fn finalization_only_processes_the_current_rejection_bucket() {
	use codec::Encode;
	new_test_ext().execute_with(|| {
		System::set_block_number(10);
		RejectionDelayBlocks::<Test, Instance1>::set(4);
		rejected_channel();
		RejectionDelayBlocks::<Test, Instance1>::set(8);
		rejected_channel();
		let future = ScheduledTransactionsForRejection::<Test, Instance1>::get(18).encode();

		EthereumIngressEgress::on_finalize(13);
		assert!(MockEgressBroadcasterEth::get_pending_api_calls().is_empty());
		assert!(!ScheduledTransactionsForRejection::<Test, Instance1>::contains_key(13));
		EthereumIngressEgress::on_finalize(14);
		assert_eq!(MockEgressBroadcasterEth::get_pending_api_calls().len(), 1);
		assert!(!ScheduledTransactionsForRejection::<Test, Instance1>::contains_key(14));
		assert!(!ScheduledTransactionsForRejection::<Test, Instance1>::contains_key(15));
		assert_eq!(ScheduledTransactionsForRejection::<Test, Instance1>::get(18).encode(), future);

		EthereumIngressEgress::on_finalize(18);
		assert_eq!(MockEgressBroadcasterEth::get_pending_api_calls().len(), 2);
		assert!(ScheduledTransactionsForRejection::<Test, Instance1>::iter().next().is_none());
	});
}

#[test]
fn rejection_retries_next_block_without_losing_future_entries() {
	new_test_ext().execute_with(|| {
		System::set_block_number(10);
		RejectionDelayBlocks::<Test, Instance1>::set(4);
		let (_, address) = rejected_channel();
		witness_rejected_deposit(address);
		System::set_block_number(11);
		witness_rejected_deposit(address);
		EthereumIngressEgress::on_finalize(14);
		assert_eq!(MockEgressBroadcasterEth::get_pending_api_calls().len(), 1);
		let pending = ScheduledTransactionsForRejection::<Test, Instance1>::get(15);
		assert_eq!(pending.len(), 2);
		assert!(!ScheduledTransactionsForRejection::<Test, Instance1>::contains_key(14));

		// Even after deployment completes, a retry cannot run before its next due block.
		let broadcast_id = BroadcastActions::<Test, Instance1>::iter_keys().next().unwrap();
		EthereumIngressEgress::on_broadcast_success(broadcast_id, 1);
		EthereumIngressEgress::on_finalize(14);
		assert_eq!(MockEgressBroadcasterEth::get_pending_api_calls().len(), 1);
		EthereumIngressEgress::on_finalize(15);
		assert!(ScheduledTransactionsForRejection::<Test, Instance1>::iter().next().is_none());
		assert_eq!(MockEgressBroadcasterEth::get_pending_api_calls().len(), 3);
		EthereumIngressEgress::on_finalize(17);
		assert_eq!(MockEgressBroadcasterEth::get_pending_api_calls().len(), 3);
	});
}

#[test]
fn cleanup_retains_queued_and_pending_fetches_and_deduplicates_retries() {
	new_test_ext().execute_with(|| {
		let (channel_id, address) = request_address_and_deposit(ALICE, EthAsset::Eth);
		let channel = DepositChannelLookup::<Test, Instance1>::get(address).unwrap();
		EthereumIngressEgress::recycle_channels(vec![address]);
		EthereumIngressEgress::recycle_channels(vec![address]);
		assert_eq!(DepositChannelLookup::<Test, Instance1>::get(address), Some(channel));
		assert!(!DepositChannelPool::<Test, Instance1>::contains_key(channel_id));
		assert_eq!(DepositChannelRecycleBlocks::<Test, Instance1>::get().len(), 1);
		EthereumIngressEgress::on_finalize(1);
		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());
		assert!(ScheduledTransactionsForRejection::<Test, Instance1>::iter().next().is_none());
		assert_eq!(MockEgressBroadcasterEth::get_pending_api_calls().len(), 1);

		// The fetch has left the queue, but its deployment callback still needs the channel.
		let pending_channel = DepositChannelLookup::<Test, Instance1>::get(address).unwrap();
		assert_eq!(pending_channel.deposit_channel.state, DeploymentStatus::Pending);
		let retry_at = DepositChannelRecycleBlocks::<Test, Instance1>::get()[0].0;
		set_eth_processed_up_to(retry_at);
		EthereumIngressEgress::on_idle(2, Weight::MAX);
		assert_eq!(DepositChannelLookup::<Test, Instance1>::get(address), Some(pending_channel));
		assert!(!DepositChannelPool::<Test, Instance1>::contains_key(channel_id));
		assert_eq!(
			DepositChannelRecycleBlocks::<Test, Instance1>::get(),
			vec![(retry_at + 50, address)]
		);

		let broadcast_id = BroadcastActions::<Test, Instance1>::iter_keys().next().unwrap();
		EthereumIngressEgress::on_broadcast_success(broadcast_id, retry_at);
		set_eth_processed_up_to(retry_at + 50);
		EthereumIngressEgress::on_idle(3, Weight::MAX);
		assert!(!DepositChannelLookup::<Test, Instance1>::contains_key(address));
		assert_eq!(
			DepositChannelPool::<Test, Instance1>::get(channel_id).unwrap().state,
			DeploymentStatus::Deployed { at_block_height: retry_at }
		);
		assert!(DepositChannelRecycleBlocks::<Test, Instance1>::get().is_empty());
	});
}

#[test]
fn batch_cleanup_retains_pending_channels_and_recycles_ready_channels() {
	new_test_ext().execute_with(|| {
		System::set_block_number(10);
		RejectionDelayBlocks::<Test, Instance1>::set(4);
		let first_rejection = rejected_channel();
		System::set_block_number(11);
		let second_rejection = rejected_channel();
		let queued_fetch = request_address_and_deposit(ALICE, EthAsset::Eth);
		let pending_deployment = empty_channel();
		let ready = empty_channel();
		let unused = empty_channel();
		for (address, state) in [
			(pending_deployment.1, DeploymentStatus::Pending),
			(ready.1, DeploymentStatus::Deployed { at_block_height: 1 }),
		] {
			DepositChannelLookup::<Test, Instance1>::mutate(address, |details| {
				details.as_mut().unwrap().deposit_channel.state = state;
			});
		}
		let recycle_at = DepositChannelRecycleBlocks::<Test, Instance1>::get()[0].0;
		set_eth_processed_up_to(recycle_at);
		EthereumIngressEgress::on_idle(11, Weight::MAX);

		for (channel_id, address) in
			[first_rejection, second_rejection, queued_fetch, pending_deployment]
		{
			assert!(DepositChannelLookup::<Test, Instance1>::contains_key(address));
			assert!(!DepositChannelPool::<Test, Instance1>::contains_key(channel_id));
		}
		assert!(!DepositChannelLookup::<Test, Instance1>::contains_key(ready.1));
		assert!(DepositChannelPool::<Test, Instance1>::contains_key(ready.0));
		assert!(!DepositChannelLookup::<Test, Instance1>::contains_key(unused.1));
		assert!(!DepositChannelPool::<Test, Instance1>::contains_key(unused.0));
		let retries = DepositChannelRecycleBlocks::<Test, Instance1>::get();
		assert_eq!(retries.len(), 4);
		assert!(retries.iter().all(|(at, _)| *at == recycle_at + 50));
		assert_eq!(ScheduledTransactionsForRejection::<Test, Instance1>::get(14).len(), 1);
		assert_eq!(ScheduledTransactionsForRejection::<Test, Instance1>::get(15).len(), 1);
	});
}

#[test]
fn cleanup_leaves_unselected_addresses_due_for_the_next_block() {
	new_test_ext().execute_with(|| {
		let first = empty_channel();
		let second = empty_channel();
		for (_, address) in [first, second] {
			DepositChannelLookup::<Test, Instance1>::mutate(address, |details| {
				details.as_mut().unwrap().deposit_channel.state =
					DeploymentStatus::Deployed { at_block_height: 1 };
			});
		}
		let queue = DepositChannelRecycleBlocks::<Test, Instance1>::get();
		let recycle_at = queue[0].0;
		set_eth_processed_up_to(recycle_at);
		let db_weight = frame_support::weights::constants::ParityDbWeight::get();

		EthereumIngressEgress::on_idle(1, db_weight.reads_writes(1, 1));
		assert_eq!(DepositChannelRecycleBlocks::<Test, Instance1>::get(), queue);
		assert!(DepositChannelLookup::<Test, Instance1>::contains_key(first.1));
		assert!(DepositChannelLookup::<Test, Instance1>::contains_key(second.1));

		// Queue bookkeeping plus the estimate for one address.
		let budget = db_weight.reads_writes(7, 4);
		EthereumIngressEgress::on_idle(2, budget);
		assert!(!DepositChannelLookup::<Test, Instance1>::contains_key(first.1));
		assert!(DepositChannelPool::<Test, Instance1>::contains_key(first.0));
		assert!(DepositChannelLookup::<Test, Instance1>::contains_key(second.1));
		assert_eq!(
			DepositChannelRecycleBlocks::<Test, Instance1>::get(),
			vec![(recycle_at, second.1)]
		);

		// The external-chain height has not advanced, but the remaining address is still due.
		EthereumIngressEgress::on_idle(3, budget);
		assert!(!DepositChannelLookup::<Test, Instance1>::contains_key(second.1));
		assert!(DepositChannelPool::<Test, Instance1>::contains_key(second.0));
		assert!(DepositChannelRecycleBlocks::<Test, Instance1>::get().is_empty());
	});
}

fn election_managed_channel() -> (ChannelId, ScriptPubkey) {
	let (channel_id, address, _, _) =
		ElectionManagedIngressEgress::request_liquidity_deposit_address(
			BROKER,
			BROKER,
			btc::Asset::Btc,
			0,
			ForeignChainAddress::Btc(ScriptPubkey::Taproot([2; 32])),
			None,
		)
		.unwrap();
	(channel_id, address.try_into().unwrap())
}

#[test]
fn election_managed_channel_cleanup_retries_without_reusing_address() {
	new_test_ext().execute_with(|| {
		let (channel_id, address) = election_managed_channel();
		assert!(!DepositChannelRecycleBlocks::<Test, Instance3>::exists());
		ScheduledEgressFetchOrTransfer::<Test, Instance3>::append(
			FetchOrTransfer::<Bitcoin>::Fetch {
				asset: btc::Asset::Btc,
				deposit_address: address.clone(),
				deposit_fetch_id: None,
				amount: 100_000,
			},
		);
		ProcessedUpTo::<Test, Instance3>::set(10);
		<ElectionManagedIngressEgress as IngressSink>::on_channel_closed(address.clone());
		assert!(DepositChannelLookup::<Test, Instance3>::contains_key(&address));
		assert_eq!(
			DepositChannelRecycleBlocks::<Test, Instance3>::get(),
			vec![(10, address.clone())]
		);

		// The queued fetch keeps the channel until a later attempt.
		ElectionManagedIngressEgress::on_idle(1, Weight::MAX);
		assert!(DepositChannelLookup::<Test, Instance3>::contains_key(&address));
		let retry_at = 10 + ChannelCleanupRetryBlocks::<Test, Instance3>::get();
		assert_eq!(
			DepositChannelRecycleBlocks::<Test, Instance3>::get(),
			vec![(retry_at, address.clone())]
		);

		ElectionManagedIngressEgress::on_finalize(1);
		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance3>::get().is_empty());
		ProcessedUpTo::<Test, Instance3>::set(retry_at);
		ElectionManagedIngressEgress::on_idle(2, Weight::MAX);
		assert!(!DepositChannelLookup::<Test, Instance3>::contains_key(&address));
		assert!(!DepositChannelPool::<Test, Instance3>::contains_key(channel_id));
		assert!(DepositChannelRecycleBlocks::<Test, Instance3>::get().is_empty());
	});
}

#[test]
fn election_managed_closures_are_cleaned_up_together_in_on_idle() {
	new_test_ext().execute_with(|| {
		let channels: Vec<_> = (0..3).map(|_| election_managed_channel()).collect();
		for (_, address) in &channels {
			<ElectionManagedIngressEgress as IngressSink>::on_channel_closed(address.clone());
			assert!(DepositChannelLookup::<Test, Instance3>::contains_key(address));
		}
		assert_eq!(DepositChannelRecycleBlocks::<Test, Instance3>::get().len(), channels.len());

		ElectionManagedIngressEgress::on_idle(1, Weight::MAX);
		for (channel_id, address) in &channels {
			assert!(!DepositChannelLookup::<Test, Instance3>::contains_key(address));
			assert!(!DepositChannelPool::<Test, Instance3>::contains_key(channel_id));
		}
		assert!(DepositChannelRecycleBlocks::<Test, Instance3>::get().is_empty());
	});
}

#[test]
fn cleanup_retry_uses_the_height_on_idle_compares_against() {
	new_test_ext().execute_with(|| {
		let (_, address) = request_address_and_deposit(ALICE, EthAsset::Eth);
		DepositChannelRecycleBlocks::<Test, Instance1>::kill();
		ProcessedUpTo::<Test, Instance1>::set(100);
		BlockHeightProvider::<MockEthereum>::set_block_height(200);

		EthereumIngressEgress::recycle_channels(vec![address]);
		assert_eq!(DepositChannelRecycleBlocks::<Test, Instance1>::get(), vec![(150, address)]);
	});
}

#[test]
fn zero_rejection_delay_is_processed_in_the_next_block() {
	new_test_ext().execute_with(|| {
		System::set_block_number(10);
		RejectionDelayBlocks::<Test, Instance1>::set(0);
		// Deposits can be processed after this pallet has finalised the current block.
		EthereumIngressEgress::on_finalize(10);
		rejected_channel();
		assert!(!ScheduledTransactionsForRejection::<Test, Instance1>::contains_key(10));

		EthereumIngressEgress::on_finalize(11);
		assert_eq!(MockEgressBroadcasterEth::get_pending_api_calls().len(), 1);
		assert!(ScheduledTransactionsForRejection::<Test, Instance1>::iter().next().is_none());
	});
}

#[test]
fn empty_cleanup_queue_is_not_written() {
	new_test_ext().execute_with(|| {
		assert!(!DepositChannelRecycleBlocks::<Test, Instance3>::exists());
		ElectionManagedIngressEgress::on_idle(1, Weight::MAX);
		assert!(!DepositChannelRecycleBlocks::<Test, Instance3>::exists());
	});
}
