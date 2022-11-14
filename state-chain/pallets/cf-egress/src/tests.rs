use crate::{mock::*, DisabledEgressAssets, FetchOrTransfer, ScheduledRequests, WeightInfo};

use cf_primitives::{chains::assets::eth, ForeignChain};
use cf_traits::{EgressApi, IngressFetchApi};

use frame_support::{assert_ok, traits::Hooks};
const ALICE_ETH_ADDRESS: EthereumAddress = [100u8; 20];
const BOB_ETH_ADDRESS: EthereumAddress = [101u8; 20];
const ETH_ETH: eth::Asset = eth::Asset::Eth;
const ETH_FLIP: eth::Asset = eth::Asset::Flip;

#[test]
fn disallowed_asset_will_not_be_batch_sent() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;

		// Cannot egress assets that are blacklisted.
		assert!(DisabledEgressAssets::<Test>::get(asset).is_none());
		assert_ok!(Egress::disable_asset_egress(Origin::root(), asset, true));
		assert!(DisabledEgressAssets::<Test>::get(asset).is_some());
		System::assert_last_event(Event::Egress(crate::Event::AssetEgressDisabled {
			asset,
			disabled: true,
		}));
		Egress::schedule_egress(asset, 1_000, ALICE_ETH_ADDRESS.into());
		Egress::on_idle(1, 1_000_000_000_000u64);

		// The egress has not been sent
		assert_eq!(
			ScheduledRequests::<Test>::get(),
			vec![FetchOrTransfer::<Ethereum>::Transfer {
				asset,
				amount: 1_000,
				to: ALICE_ETH_ADDRESS.into(),
			}]
		);

		// re-enable the asset for Egress
		assert_ok!(Egress::disable_asset_egress(Origin::root(), asset, false));
		assert!(DisabledEgressAssets::<Test>::get(asset).is_none());
		System::assert_last_event(Event::Egress(crate::Event::AssetEgressDisabled {
			asset,
			disabled: false,
		}));

		Egress::on_idle(1, 1_000_000_000_000u64);

		// The egress should be sent now
		assert!(ScheduledRequests::<Test>::get().is_empty());
	});
}

#[test]
fn can_schedule_egress_to_batch() {
	new_test_ext().execute_with(|| {
		Egress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into());
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			asset: ETH_ETH,
			amount: 2_000,
			egress_address: ALICE_ETH_ADDRESS.into(),
		}));

		Egress::schedule_egress(ETH_FLIP, 3_000, BOB_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_FLIP, 4_000, BOB_ETH_ADDRESS.into());
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			asset: ETH_FLIP,
			amount: 4_000,
			egress_address: BOB_ETH_ADDRESS.into(),
		}));

		assert_eq!(
			ScheduledRequests::<Test>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_ETH,
					amount: 1_000,
					to: ALICE_ETH_ADDRESS.into(),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_ETH,
					amount: 2_000,
					to: ALICE_ETH_ADDRESS.into(),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 3_000,
					to: BOB_ETH_ADDRESS.into(),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 4_000,
					to: BOB_ETH_ADDRESS.into(),
				},
			]
		);
	});
}

#[test]
fn can_schedule_ingress_fetch() {
	new_test_ext().execute_with(|| {
		assert!(ScheduledRequests::<Test>::get().is_empty());

		Egress::schedule_ingress_fetch(vec![(ETH_ETH, 1u64), (ETH_ETH, 2u64), (ETH_FLIP, 3u64)]);
		assert_eq!(
			ScheduledRequests::<Test>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 1u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 2u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 3u64, asset: ETH_FLIP },
			]
		);

		System::assert_last_event(Event::Egress(crate::Event::IngressFetchesScheduled {
			fetches_added: 3u32,
		}));

		Egress::schedule_ingress_fetch(vec![(ETH_ETH, 4u64)]);

		assert_eq!(
			ScheduledRequests::<Test>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 1u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 2u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 3u64, asset: ETH_FLIP },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 4u64, asset: ETH_ETH },
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
		Egress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS.into());
		Egress::schedule_ingress_fetch(vec![
			(ETH_ETH, 1),
			(ETH_ETH, 2),
			(ETH_ETH, 3),
			(ETH_ETH, 4),
		]);

		Egress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS.into());
		Egress::schedule_ingress_fetch(vec![(ETH_FLIP, 5)]);

		// Take all scheduled Egress and Broadcast as batch
		Egress::on_idle(1, 1_000_000_000_000u64);

		System::assert_has_event(Event::Egress(crate::Event::BatchBroadcastRequested {
			fetch_batch_size: 5u32,
			egress_batch_size: 8u32,
		}));

		assert!(ScheduledRequests::<Test>::get().is_empty());
	});
}

#[test]
fn can_manually_send_batch_all() {
	new_test_ext().execute_with(|| {
		Egress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_ingress_fetch(vec![(ETH_ETH, 1), (ETH_FLIP, 2)]);
		Egress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS.into());

		Egress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS.into());
		Egress::schedule_ingress_fetch(vec![(ETH_ETH, 3), (ETH_FLIP, 4)]);

		// Send only 2 requests
		assert_ok!(Egress::send_scheduled_batch_for_chain(
			Origin::root(),
			ForeignChain::Ethereum,
			Some(2)
		));
		System::assert_has_event(Event::Egress(crate::Event::BatchBroadcastRequested {
			fetch_batch_size: 1u32,
			egress_batch_size: 1u32,
		}));
		assert_eq!(ScheduledRequests::<Test>::decode_len(), Some(10));

		// send all remaining requests
		assert_ok!(Egress::send_scheduled_batch_for_chain(
			Origin::root(),
			ForeignChain::Ethereum,
			None
		));

		System::assert_has_event(Event::Egress(crate::Event::BatchBroadcastRequested {
			fetch_batch_size: 3u32,
			egress_batch_size: 7u32,
		}));

		assert!(ScheduledRequests::<Test>::get().is_empty());
	});
}

#[test]
fn on_idle_batch_size_is_limited_by_weight() {
	new_test_ext().execute_with(|| {
		Egress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_ingress_fetch(vec![(ETH_ETH, 1), (ETH_ETH, 2)]);
		Egress::schedule_egress(ETH_FLIP, 3_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_FLIP, 4_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_ingress_fetch(vec![(ETH_FLIP, 3), (ETH_FLIP, 4)]);

		// There's enough weights for 3 transactions, which are taken in FIFO order.
		Egress::on_idle(1, <Test as crate::Config>::WeightInfo::send_batch(3) + 1);

		System::assert_has_event(Event::Egress(crate::Event::BatchBroadcastRequested {
			fetch_batch_size: 1u32,
			egress_batch_size: 2u32,
		}));

		// Send another 3 requests.
		Egress::on_idle(1, <Test as crate::Config>::WeightInfo::send_batch(3) + 1);

		System::assert_has_event(Event::Egress(crate::Event::BatchBroadcastRequested {
			fetch_batch_size: 1u32,
			egress_batch_size: 2u32,
		}));

		assert_eq!(
			ScheduledRequests::<Test>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 5_000,
					to: ALICE_ETH_ADDRESS.into(),
				},
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 3u64, asset: ETH_FLIP },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 4u64, asset: ETH_FLIP },
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
			<Test as crate::Config>::WeightInfo::send_batch(0)
		);

		// Blacklist Eth for Ethereum.
		let asset = ETH_ETH;
		assert_ok!(Egress::disable_asset_egress(Origin::root(), asset, true));

		Egress::schedule_egress(asset, 1_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(asset, 2_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(asset, 3_000, ALICE_ETH_ADDRESS.into());
		Egress::schedule_egress(asset, 4_000, ALICE_ETH_ADDRESS.into());
		assert_eq!(
			Egress::on_idle(1, 1_000_000_000_000_000u64),
			<Test as crate::Config>::WeightInfo::send_batch(0)
		);

		assert_eq!(ScheduledRequests::<Test>::decode_len(), Some(4));
	});
}
