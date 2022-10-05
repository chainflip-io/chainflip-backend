use crate::{mock::*, DisabledEgressAssets, EthereumScheduledIngressFetch, ScheduledEgress};

use cf_chains::eth::ingress_address::get_salt;
use cf_primitives::{
	FetchParameter, ForeignChain, ForeignChainAddress, ForeignChainAsset, ETHEREUM_ETH_ADDRESS,
};
use cf_traits::{EgressApi, IngressFetchApi};

use frame_support::{assert_noop, assert_ok, traits::Hooks};
const ALICE_ETH_ADDRESS: EthereumAddress = [100u8; 20];
const BOB_ETH_ADDRESS: EthereumAddress = [101u8; 20];
const ETH_ETH: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth };
const ETH_FLIP: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Flip };

#[test]
fn can_only_schedule_egress_allowed_asset() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;
		assert!(Egress::get_asset_ethereum_address(asset.asset).is_some());

		// Cannot egress assets that are blacklisted.
		assert!(DisabledEgressAssets::<Test>::get(asset).is_none());
		assert_ok!(Egress::disable_asset_egress(Origin::root(), asset, true));
		assert!(DisabledEgressAssets::<Test>::get(asset).is_some());
		System::assert_last_event(Event::Egress(crate::Event::AssetEgressDisabled {
			asset,
			disabled: true,
		}));
		assert_noop!(
			Egress::schedule_egress(asset, 1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
			crate::Error::<Test>::AssetEgressDisabled
		);

		// re-enable the asset for Egress
		assert_ok!(Egress::disable_asset_egress(Origin::root(), asset, false));
		assert!(DisabledEgressAssets::<Test>::get(asset).is_none());
		System::assert_last_event(Event::Egress(crate::Event::AssetEgressDisabled {
			asset,
			disabled: false,
		}));
		assert_ok!(Egress::schedule_egress(
			asset,
			1_000,
			ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)
		),);
	});
}

#[test]
fn can_schedule_egress_to_batch() {
	new_test_ext().execute_with(|| {
		assert_ok!(Egress::schedule_egress(
			ETH_ETH,
			1_000,
			ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_ETH,
			2_000,
			ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
		));
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			foreign_asset: ETH_ETH,
			amount: 2_000,
			egress_address: ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
		}));

		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			3_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			4_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			foreign_asset: ETH_FLIP,
			amount: 4_000,
			egress_address: ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		}));

		assert_eq!(
			ScheduledEgress::<Test>::get(ETH_ETH),
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS))
			]
		);
		assert_eq!(
			ScheduledEgress::<Test>::get(ETH_FLIP),
			vec![
				(3_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
				(4_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS))
			]
		);
	});
}

#[test]
fn can_schedule_ingress_fetchegress_to_batch() {
	new_test_ext().execute_with(|| {
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Dot).is_empty());

		Egress::schedule_ingress_fetch(vec![
			(Asset::Eth, FetchParameter::Eth(1)),
			(Asset::Eth, FetchParameter::Eth(2)),
			(Asset::Dot, FetchParameter::Eth(3)),
		]);

		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth), vec![1, 2]);
		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Dot), vec![3]);

		System::assert_last_event(Event::Egress(crate::Event::EthereumIngressFetchesScheduled {
			fetches_added: 3u32,
		}));

		Egress::schedule_ingress_fetch(vec![(Asset::Eth, FetchParameter::Eth(4))]);

		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth), vec![1, 2, 4]);
		System::assert_last_event(Event::Egress(crate::Event::EthereumIngressFetchesScheduled {
			fetches_added: 1u32,
		}));
	});
}

#[test]
fn on_idle_can_send_batch_all() {
	new_test_ext().execute_with(|| {
		ScheduledEgress::<Test>::insert(
			ETH_ETH,
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(3_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
				(4_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
			],
		);
		EthereumScheduledIngressFetch::<Test>::insert(Asset::Eth, vec![1, 2, 3, 4]);

		ScheduledEgress::<Test>::insert(
			ETH_FLIP,
			vec![
				(5_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(6_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(7_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
				(8_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
			],
		);
		EthereumScheduledIngressFetch::<Test>::insert(Asset::Flip, vec![5]);
		EthereumScheduledIngressFetch::<Test>::insert(Asset::Usdc, vec![6, 7]);

		assert!(LastEgressSent::get().is_empty());

		// Take all scheduled Egress and Broadcast as batch
		Egress::on_idle(1, 1_000_000_000_000u64);

		// The order the assets are iterated are random but deterministic.
		// In this case ETH batch is sent first, followed by FLIP batch.
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
				(get_salt(1), ETHEREUM_ETH_ADDRESS),
				(get_salt(2), ETHEREUM_ETH_ADDRESS),
				(get_salt(3), ETHEREUM_ETH_ADDRESS),
				(get_salt(4), ETHEREUM_ETH_ADDRESS),
				(get_salt(5), ETHEREUM_FLIP_ADDRESS)
			]
		);

		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			foreign_assets: vec![ETH_ETH, ETH_FLIP],
			egress_batch_size: 8u32,
			fetch_batch_size: 5u32,
		}));

		assert!(ScheduledEgress::<Test>::get(ETH_ETH).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth).is_empty());
		assert!(ScheduledEgress::<Test>::get(ETH_FLIP).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Flip).is_empty());

		// Only Eth and Flip are sent, Usdc is unaffected.
		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Usdc), vec![6, 7]);
	});
}

#[test]
fn can_manually_send_batch_all() {
	new_test_ext().execute_with(|| {
		ScheduledEgress::<Test>::insert(
			ETH_ETH,
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(3_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
				(4_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
			],
		);
		EthereumScheduledIngressFetch::<Test>::insert(Asset::Eth, vec![1]);

		ScheduledEgress::<Test>::insert(
			ETH_FLIP,
			vec![
				(5_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(6_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(7_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
				(8_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
			],
		);
		EthereumScheduledIngressFetch::<Test>::insert(Asset::Flip, vec![2]);

		assert_ok!(Egress::send_scheduled_egress_for_asset(Origin::root(), ETH_ETH));

		// Only `ETH_ETH` are egressed
		assert_eq!(
			LastEgressSent::get(),
			vec![
				(ETHEREUM_ETH_ADDRESS, 1000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 2000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 3000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 4000u128, BOB_ETH_ADDRESS),
			]
		);
		assert_eq!(LastFetchesSent::get(), vec![(get_salt(1), ETHEREUM_ETH_ADDRESS),],);
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			foreign_assets: vec![ETH_ETH],
			egress_batch_size: 4u32,
			fetch_batch_size: 1u32,
		}));

		assert!(ScheduledEgress::<Test>::get(ETH_ETH).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth).is_empty());
		assert!(!ScheduledEgress::<Test>::get(ETH_FLIP).is_empty());
		assert!(!EthereumScheduledIngressFetch::<Test>::get(Asset::Flip).is_empty());
	});
}
