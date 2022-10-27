use crate::{
	mock::*, EthereumDisabledEgressAssets, EthereumFetchParam, EthereumRequest,
	EthereumScheduledRequests, EthereumTransferParam, WeightInfo,
};

use cf_primitives::{ForeignChain, ForeignChainAddress, ForeignChainAsset, ETHEREUM_ETH_ADDRESS};
use cf_traits::{EgressApi, IngressFetchApi};

use frame_support::{assert_ok, traits::Hooks};
const ALICE_ETH_ADDRESS: EthereumAddress = [100u8; 20];
const BOB_ETH_ADDRESS: EthereumAddress = [101u8; 20];
const ETH_ETH: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth };
const ETH_FLIP: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Flip };

#[test]
fn disallowed_asset_will_not_be_batch_sent() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;
		assert!(Egress::get_ethereum_asset_identifier(asset.asset).is_some());

		// Cannot egress assets that are blacklisted.
		assert!(EthereumDisabledEgressAssets::<Test>::get(asset.asset).is_none());
		assert_ok!(Egress::disable_asset_egress(Origin::root(), asset, true));
		assert!(EthereumDisabledEgressAssets::<Test>::get(asset.asset).is_some());
		System::assert_last_event(Event::Egress(crate::Event::AssetEgressDisabled {
			foreign_asset: asset,
			disabled: true,
		}));
		Egress::schedule_egress(asset, 1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		LastEgressSent::set(vec![]);
		Egress::on_idle(1, 1_000_000_000_000u64);

		// The egress has not been sent
		assert!(LastEgressSent::get().is_empty());
		assert_eq!(
			EthereumScheduledRequests::<Test>::get(),
			vec![EthereumRequest::Egress(EthereumTransferParam {
				asset: Asset::Eth,
				amount: 1_000,
				to: ALICE_ETH_ADDRESS,
			})]
		);

		// re-enable the asset for Egress
		assert_ok!(Egress::disable_asset_egress(Origin::root(), asset, false));
		assert!(EthereumDisabledEgressAssets::<Test>::get(Asset::Eth).is_none());
		System::assert_last_event(Event::Egress(crate::Event::AssetEgressDisabled {
			foreign_asset: asset,
			disabled: false,
		}));

		LastEgressSent::set(vec![]);
		Egress::on_idle(1, 1_000_000_000_000u64);

		// The egress should be sent now
		assert_eq!(LastEgressSent::get(), vec![(ETHEREUM_ETH_ADDRESS, 1_000, ALICE_ETH_ADDRESS)]);
		assert!(EthereumScheduledRequests::<Test>::get().is_empty());
	});
}

#[test]
fn can_schedule_egress_to_batch() {
	new_test_ext().execute_with(|| {
		Egress::schedule_egress(ETH_ETH, 1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			foreign_asset: ETH_ETH,
			amount: 2_000,
			egress_address: ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
		}));

		Egress::schedule_egress(ETH_FLIP, 3_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 4_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			foreign_asset: ETH_FLIP,
			amount: 4_000,
			egress_address: ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		}));

		assert_eq!(
			EthereumScheduledRequests::<Test>::get(),
			vec![
				EthereumRequest::Egress(EthereumTransferParam {
					asset: Asset::Eth,
					amount: 1_000,
					to: ALICE_ETH_ADDRESS,
				}),
				EthereumRequest::Egress(EthereumTransferParam {
					asset: Asset::Eth,
					amount: 2_000,
					to: ALICE_ETH_ADDRESS,
				}),
				EthereumRequest::Egress(EthereumTransferParam {
					asset: Asset::Flip,
					amount: 3_000,
					to: BOB_ETH_ADDRESS,
				}),
				EthereumRequest::Egress(EthereumTransferParam {
					asset: Asset::Flip,
					amount: 4_000,
					to: BOB_ETH_ADDRESS,
				})
			]
		);
	});
}

#[test]
fn can_schedule_ethereum_ingress_fetch() {
	new_test_ext().execute_with(|| {
		assert!(EthereumScheduledRequests::<Test>::get().is_empty());

		Egress::schedule_ethereum_ingress_fetch(vec![
			(Asset::Eth, 1u64),
			(Asset::Eth, 2u64),
			(Asset::Dot, 3u64),
		]);
		assert_eq!(
			EthereumScheduledRequests::<Test>::get(),
			vec![
				EthereumRequest::Fetch(EthereumFetchParam { intent_id: 1u64, asset: Asset::Eth }),
				EthereumRequest::Fetch(EthereumFetchParam { intent_id: 2u64, asset: Asset::Eth }),
				EthereumRequest::Fetch(EthereumFetchParam { intent_id: 3u64, asset: Asset::Dot })
			]
		);

		System::assert_last_event(Event::Egress(crate::Event::IngressFetchesScheduled {
			fetches_added: 3u32,
		}));

		Egress::schedule_ethereum_ingress_fetch(vec![(Asset::Eth, 4u64)]);

		assert_eq!(
			EthereumScheduledRequests::<Test>::get(),
			vec![
				EthereumRequest::Fetch(EthereumFetchParam { intent_id: 1u64, asset: Asset::Eth }),
				EthereumRequest::Fetch(EthereumFetchParam { intent_id: 2u64, asset: Asset::Eth }),
				EthereumRequest::Fetch(EthereumFetchParam { intent_id: 3u64, asset: Asset::Dot }),
				EthereumRequest::Fetch(EthereumFetchParam { intent_id: 4u64, asset: Asset::Eth })
			]
		);
		System::assert_last_event(Event::Egress(crate::Event::IngressFetchesScheduled {
			fetches_added: 1u32,
		}));
	});
}

#[test]
fn on_idle_can_send_batch_all() {
	new_test_ext().execute_with(|| {
		Egress::schedule_egress(ETH_ETH, 1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 3_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 4_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_ethereum_ingress_fetch(vec![
			(Asset::Eth, 1),
			(Asset::Eth, 2),
			(Asset::Eth, 3),
			(Asset::Eth, 4),
		]);

		Egress::schedule_egress(ETH_FLIP, 5_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 6_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 7_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 8_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_ethereum_ingress_fetch(vec![
			(Asset::Flip, 5),
			// Unsupported items should never be added. But if they are, system should not panic
			(Asset::Usdc, 6),
		]);

		assert!(LastEgressSent::get().is_empty());

		// Take all scheduled Egress and Broadcast as batch
		Egress::on_idle(1, 1_000_000_000_000u64);

		// The requests are processed FIFO
		assert_eq!(
			LastEgressSent::get(),
			vec![
				(ETHEREUM_ETH_ADDRESS, 1_000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 2_000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 3_000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 4_000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 5_000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 6_000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 7_000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 8_000u128, BOB_ETH_ADDRESS),
			]
		);

		assert_eq!(
			LastFetchesSent::get(),
			vec![
				(1u64, ETHEREUM_ETH_ADDRESS),
				(2u64, ETHEREUM_ETH_ADDRESS),
				(3u64, ETHEREUM_ETH_ADDRESS),
				(4u64, ETHEREUM_ETH_ADDRESS),
				(5u64, ETHEREUM_FLIP_ADDRESS),
			]
		);

		System::assert_has_event(Event::Egress(crate::Event::EthereumBatchBroadcastRequested {
			fetch_batch_size: 5u32,
			egress_batch_size: 8u32,
		}));

		assert!(EthereumScheduledRequests::<Test>::get().is_empty());
	});
}

#[test]
fn can_manually_send_batch_all() {
	new_test_ext().execute_with(|| {
		Egress::schedule_egress(ETH_ETH, 1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_ethereum_ingress_fetch(vec![(Asset::Eth, 1), (Asset::Flip, 2)]);
		Egress::schedule_egress(ETH_ETH, 2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 3_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 4_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));

		Egress::schedule_egress(ETH_FLIP, 5_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 6_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 7_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 8_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_ethereum_ingress_fetch(vec![(Asset::Eth, 3), (Asset::Flip, 4)]);

		LastEgressSent::set(vec![]);
		LastFetchesSent::set(vec![]);

		// Send only 2 requests
		assert_ok!(Egress::send_scheduled_batch_for_chain(
			Origin::root(),
			ForeignChain::Ethereum,
			Some(2)
		));
		assert_eq!(
			LastEgressSent::get(),
			vec![(ETHEREUM_ETH_ADDRESS, 1000u128, ALICE_ETH_ADDRESS),]
		);
		assert_eq!(LastFetchesSent::get(), vec![(1u64, ETHEREUM_ETH_ADDRESS)]);
		System::assert_has_event(Event::Egress(crate::Event::EthereumBatchBroadcastRequested {
			fetch_batch_size: 1u32,
			egress_batch_size: 1u32,
		}));
		assert_eq!(EthereumScheduledRequests::<Test>::decode_len(), Some(10));

		// send all remaining requests
		assert_ok!(Egress::send_scheduled_batch_for_chain(
			Origin::root(),
			ForeignChain::Ethereum,
			None
		));
		assert_eq!(
			LastEgressSent::get(),
			vec![
				(ETHEREUM_ETH_ADDRESS, 2000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 3000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 4000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 5000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 6000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 7000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 8000u128, BOB_ETH_ADDRESS),
			]
		);
		assert_eq!(
			LastFetchesSent::get(),
			vec![
				(2u64, ETHEREUM_FLIP_ADDRESS),
				(3u64, ETHEREUM_ETH_ADDRESS),
				(4u64, ETHEREUM_FLIP_ADDRESS)
			]
		);
		System::assert_has_event(Event::Egress(crate::Event::EthereumBatchBroadcastRequested {
			fetch_batch_size: 3u32,
			egress_batch_size: 7u32,
		}));

		assert!(EthereumScheduledRequests::<Test>::get().is_empty());
	});
}

#[test]
fn on_idle_batch_size_is_limited_by_weight() {
	new_test_ext().execute_with(|| {
		Egress::schedule_egress(ETH_ETH, 1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_ethereum_ingress_fetch(vec![(Asset::Eth, 1), (Asset::Eth, 2)]);
		Egress::schedule_egress(ETH_FLIP, 3_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 4_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 5_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_ethereum_ingress_fetch(vec![(Asset::Flip, 3), (Asset::Flip, 4)]);

		LastEgressSent::set(vec![]);
		LastFetchesSent::set(vec![]);

		// There's enough weights for 3 transactions, which are taken in FIFO order.
		Egress::on_idle(1, <Test as crate::Config>::WeightInfo::send_ethereum_batch(3) + 1);

		assert_eq!(LastFetchesSent::get(), vec![(1, ETHEREUM_ETH_ADDRESS)]);
		assert_eq!(
			LastEgressSent::get(),
			vec![
				(ETHEREUM_ETH_ADDRESS, 1000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 2000u128, ALICE_ETH_ADDRESS),
			]
		);

		System::assert_has_event(Event::Egress(crate::Event::EthereumBatchBroadcastRequested {
			fetch_batch_size: 1u32,
			egress_batch_size: 2u32,
		}));

		// Send another 3 requests.
		Egress::on_idle(1, <Test as crate::Config>::WeightInfo::send_ethereum_batch(3) + 1);

		assert_eq!(
			LastEgressSent::get(),
			vec![
				(ETHEREUM_FLIP_ADDRESS, 3000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 4000u128, ALICE_ETH_ADDRESS),
			]
		);
		assert_eq!(LastFetchesSent::get(), vec![(2, ETHEREUM_ETH_ADDRESS)]);
		System::assert_has_event(Event::Egress(crate::Event::EthereumBatchBroadcastRequested {
			fetch_batch_size: 1u32,
			egress_batch_size: 2u32,
		}));

		assert_eq!(
			EthereumScheduledRequests::<Test>::get(),
			vec![
				EthereumRequest::Egress(EthereumTransferParam {
					asset: Asset::Flip,
					amount: 5_000,
					to: ALICE_ETH_ADDRESS,
				}),
				EthereumRequest::Fetch(EthereumFetchParam { intent_id: 3u64, asset: Asset::Flip }),
				EthereumRequest::Fetch(EthereumFetchParam { intent_id: 4u64, asset: Asset::Flip })
			]
		);
	});
}

#[test]
fn on_idle_does_nothing_if_nothing_to_send() {
	new_test_ext().execute_with(|| {
		// Does not panic if request queue is empty.
		assert_eq!(
			Egress::on_idle(1, 1_000_000_000_000_000u64),
			<Test as crate::Config>::WeightInfo::send_ethereum_batch(0)
		);

		// Blacklist Eth for Ethereum.
		let asset = ETH_ETH;
		assert_ok!(Egress::disable_asset_egress(Origin::root(), asset, true));

		Egress::schedule_egress(asset, 1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(asset, 2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(asset, 3_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(asset, 4_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		assert_eq!(
			Egress::on_idle(1, 1_000_000_000_000_000u64),
			<Test as crate::Config>::WeightInfo::send_ethereum_batch(0)
		);

		assert_eq!(EthereumScheduledRequests::<Test>::decode_len(), Some(4));
	});
}
