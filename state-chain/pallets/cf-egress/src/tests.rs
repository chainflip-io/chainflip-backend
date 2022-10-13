use crate::{
	mock::*, EthereumDisabledEgressAssets, EthereumScheduledEgress, EthereumScheduledIngressFetch,
	WeightInfo,
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
		assert!(Egress::get_asset_ethereum_address(asset.asset).is_some());

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
			EthereumScheduledEgress::<Test>::get(Asset::Eth),
			vec![(1_000, ALICE_ETH_ADDRESS)]
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
		assert!(EthereumScheduledEgress::<Test>::get(Asset::Eth).is_empty());
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
			EthereumScheduledEgress::<Test>::get(Asset::Eth),
			vec![(1_000, ALICE_ETH_ADDRESS), (2_000, ALICE_ETH_ADDRESS)]
		);
		assert_eq!(
			EthereumScheduledEgress::<Test>::get(Asset::Flip),
			vec![(3_000, BOB_ETH_ADDRESS), (4_000, BOB_ETH_ADDRESS)]
		);
	});
}

#[test]
fn can_schedule_ethereum_ingress_fetch() {
	new_test_ext().execute_with(|| {
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Dot).is_empty());

		Egress::schedule_ethereum_ingress_fetch(vec![
			(Asset::Eth, 1u64),
			(Asset::Eth, 2u64),
			(Asset::Dot, 3u64),
		]);

		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth), vec![1, 2]);
		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Dot), vec![3]);

		System::assert_last_event(Event::Egress(crate::Event::IngressFetchesScheduled {
			fetches_added: 3u32,
		}));

		Egress::schedule_ethereum_ingress_fetch(vec![(Asset::Eth, 4u64)]);

		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth), vec![1, 2, 4]);
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

		// The order the assets are iterated are random but deterministic.
		assert_eq!(
			LastEgressSent::get(),
			vec![
				(ETHEREUM_FLIP_ADDRESS, 5_000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 6_000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 7_000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 8_000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 1_000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 2_000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 3_000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 4_000u128, BOB_ETH_ADDRESS),
			]
		);

		assert_eq!(
			LastFetchesSent::get(),
			vec![
				(5u64, ETHEREUM_FLIP_ADDRESS),
				(1u64, ETHEREUM_ETH_ADDRESS),
				(2u64, ETHEREUM_ETH_ADDRESS),
				(3u64, ETHEREUM_ETH_ADDRESS),
				(4u64, ETHEREUM_ETH_ADDRESS),
			]
		);

		System::assert_has_event(Event::Egress(crate::Event::EthereumBatchBroadcastRequested {
			fetch_batch_size: 5u32,
			egress_batch_size: 8u32,
		}));

		assert!(EthereumScheduledEgress::<Test>::get(Asset::Eth).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth).is_empty());
		assert!(EthereumScheduledEgress::<Test>::get(Asset::Flip).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Flip).is_empty());

		// Unsupported assets are not sent.
		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Usdc), vec![(6)]);
	});
}

#[test]
fn can_manually_send_batch_all() {
	new_test_ext().execute_with(|| {
		Egress::schedule_egress(ETH_ETH, 1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 3_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 4_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));

		Egress::schedule_egress(ETH_FLIP, 5_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 6_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 7_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 8_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS));
		Egress::schedule_ethereum_ingress_fetch(vec![(Asset::Eth, 1), (Asset::Flip, 2)]);

		LastEgressSent::set(vec![]);
		LastFetchesSent::set(vec![]);
		assert_ok!(Egress::send_scheduled_batch_for_chain(Origin::root(), ForeignChain::Ethereum));

		assert_eq!(
			LastEgressSent::get(),
			vec![
				(ETHEREUM_FLIP_ADDRESS, 5000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 6000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 7000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 8000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 1000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 2000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 3000u128, BOB_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 4000u128, BOB_ETH_ADDRESS),
			]
		);
		assert_eq!(
			LastFetchesSent::get(),
			vec![(2u64, ETHEREUM_FLIP_ADDRESS), (1u64, ETHEREUM_ETH_ADDRESS)]
		);
		System::assert_has_event(Event::Egress(crate::Event::EthereumBatchBroadcastRequested {
			fetch_batch_size: 2u32,
			egress_batch_size: 8u32,
		}));

		assert!(EthereumScheduledEgress::<Test>::get(Asset::Eth).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth).is_empty());
		assert!(EthereumScheduledEgress::<Test>::get(Asset::Flip).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Flip).is_empty());
	});
}

#[test]
fn on_idle_batch_size_is_limited_by_weight() {
	new_test_ext().execute_with(|| {
		Egress::schedule_egress(ETH_ETH, 1_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_ETH, 2_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 3_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));
		Egress::schedule_egress(ETH_FLIP, 4_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS));

		Egress::schedule_ethereum_ingress_fetch(vec![
			(Asset::Eth, 1),
			(Asset::Eth, 2),
			(Asset::Flip, 3),
			(Asset::Flip, 4),
		]);

		LastEgressSent::set(vec![]);
		LastFetchesSent::set(vec![]);

		// There's enough weights for 3 Egress and 0 fetches
		Egress::on_idle(1, <Test as crate::Config>::WeightInfo::send_ethereum_batch(0, 3) + 1);

		assert_eq!(
			LastEgressSent::get(),
			vec![
				(ETHEREUM_FLIP_ADDRESS, 3000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_FLIP_ADDRESS, 4000u128, ALICE_ETH_ADDRESS),
				(ETHEREUM_ETH_ADDRESS, 1000u128, ALICE_ETH_ADDRESS),
			]
		);
		assert!(LastFetchesSent::get().is_empty());
		System::assert_has_event(Event::Egress(crate::Event::EthereumBatchBroadcastRequested {
			fetch_batch_size: 0u32,
			egress_batch_size: 3u32,
		}));

		assert!(EthereumScheduledEgress::<Test>::get(Asset::Flip).is_empty());
		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Flip), vec![3, 4]);
		assert_eq!(
			EthereumScheduledEgress::<Test>::get(Asset::Eth),
			vec![(2000, ALICE_ETH_ADDRESS)]
		);
		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth), vec![1, 2]);

		// Send the last Egress and 3 fetches
		Egress::on_idle(1, <Test as crate::Config>::WeightInfo::send_ethereum_batch(3, 1) + 1);

		assert_eq!(
			LastEgressSent::get(),
			vec![(ETHEREUM_ETH_ADDRESS, 2000u128, ALICE_ETH_ADDRESS),]
		);
		assert_eq!(
			LastFetchesSent::get(),
			vec![(3, ETHEREUM_FLIP_ADDRESS), (4, ETHEREUM_FLIP_ADDRESS), (1, ETHEREUM_ETH_ADDRESS)]
		);
		System::assert_has_event(Event::Egress(crate::Event::EthereumBatchBroadcastRequested {
			fetch_batch_size: 3u32,
			egress_batch_size: 1u32,
		}));

		assert!(EthereumScheduledEgress::<Test>::get(Asset::Flip).is_empty());
		assert!(EthereumScheduledIngressFetch::<Test>::get(Asset::Flip).is_empty());
		assert!(EthereumScheduledEgress::<Test>::get(Asset::Eth).is_empty());
		assert_eq!(EthereumScheduledIngressFetch::<Test>::get(Asset::Eth), vec![2]);
	});
}
