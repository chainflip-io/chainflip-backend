use crate::{mock::*, DisabledEgressAssets, EthereumScheduledIngressFetch, ScheduledEgress};

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
		assert_ok!(Egress::schedule_egress(
			asset,
			1_000,
			ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)
		));
		LastEgressSent::set(vec![]);
		Egress::on_idle(1, 1_000_000_000_000u64);

		// The egress has not been sent
		assert!(LastEgressSent::get().is_empty());
		assert_eq!(
			ScheduledEgress::<Test>::get(ETH_ETH),
			vec![(1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS))]
		);

		// re-enable the asset for Egress
		assert_ok!(Egress::disable_asset_egress(Origin::root(), asset, false));
		assert!(DisabledEgressAssets::<Test>::get(asset).is_none());
		System::assert_last_event(Event::Egress(crate::Event::AssetEgressDisabled {
			asset,
			disabled: false,
		}));

		LastEgressSent::set(vec![]);
		Egress::on_idle(1, 1_000_000_000_000u64);

		// The egress should be sent now
		assert_eq!(LastEgressSent::get(), vec![(ETHEREUM_ETH_ADDRESS, 1_000, ALICE_ETH_ADDRESS)]);
		assert!(ScheduledEgress::<Test>::get(ETH_ETH).is_empty());
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
fn can_schedule_ingress_fetch() {
	new_test_ext().execute_with(|| {
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Dot).is_empty());

		Egress::schedule_ingress_fetch(vec![
			(Asset::Eth, 1u64),
			(Asset::Eth, 2u64),
			(Asset::Dot, 3u64),
		]);

		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth), vec![1, 2]);
		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Dot), vec![3]);

		System::assert_last_event(Event::Egress(crate::Event::IngressFetchesScheduled {
			fetches_added: 3u32,
		}));

		Egress::schedule_ingress_fetch(vec![(Asset::Eth, 4u64)]);

		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth), vec![1, 2, 4]);
		System::assert_last_event(Event::Egress(crate::Event::IngressFetchesScheduled {
			fetches_added: 1u32,
		}));
	});
}

#[test]
fn on_idle_can_send_batch_all() {
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
		assert_ok!(Egress::schedule_egress(
			ETH_ETH,
			3_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_ETH,
			4_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));
		Egress::schedule_ingress_fetch(vec![
			(Asset::Eth, 1),
			(Asset::Eth, 2),
			(Asset::Eth, 3),
			(Asset::Eth, 4),
		]);

		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			5_000,
			ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			6_000,
			ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			7_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			8_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));
		Egress::schedule_ingress_fetch(vec![(Asset::Flip, 5)]);
		Egress::schedule_ingress_fetch(vec![(Asset::Usdc, 6), (Asset::Usdc, 7)]);

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
				(1u64, ETHEREUM_ETH_ADDRESS),
				(2u64, ETHEREUM_ETH_ADDRESS),
				(3u64, ETHEREUM_ETH_ADDRESS),
				(4u64, ETHEREUM_ETH_ADDRESS),
				(5u64, ETHEREUM_FLIP_ADDRESS)
			]
		);

		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcastRequested {
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
		assert_ok!(Egress::schedule_egress(
			ETH_ETH,
			3_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_ETH,
			4_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));

		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			5_000,
			ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			6_000,
			ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			7_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));
		assert_ok!(Egress::schedule_egress(
			ETH_FLIP,
			8_000,
			ForeignChainAddress::Eth(BOB_ETH_ADDRESS),
		));
		Egress::schedule_ingress_fetch(vec![(Asset::Eth, 1), (Asset::Flip, 2)]);

		LastEgressSent::set(vec![]);
		LastFetchesSent::set(vec![]);
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
		assert_eq!(LastFetchesSent::get(), vec![(1u64, ETHEREUM_ETH_ADDRESS),],);
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcastRequested {
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
