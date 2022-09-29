use crate::{mock::*, DisabledEgressAssets, ScheduledEgress};

use cf_primitives::{ForeignChain, ForeignChainAddress, ForeignChainAsset};
use cf_traits::EgressApi;

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

		ScheduledEgress::<Test>::insert(
			ETH_FLIP,
			vec![
				(5_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(6_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(7_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
				(8_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
			],
		);

		assert_eq!(LastEgressSent::get(), vec![]);

		// Take all scheduled Egress and Broadcast as batch
		Egress::on_idle(1, 1_000_000_000_000u64);

		// The order the assets are iterated are random but deterministic.
		// In this case ETH batch is sent first, followed by FLIP batch.
		assert_eq!(
			LastEgressSent::get(),
			vec![
				([0x00; 20], 5000u128, ALICE_ETH_ADDRESS),
				([0x00; 20], 6000u128, ALICE_ETH_ADDRESS),
				([0x00; 20], 7000u128, BOB_ETH_ADDRESS),
				([0x00; 20], 8000u128, BOB_ETH_ADDRESS),
			]
		);
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			foreign_asset: ETH_ETH,
			batch_size: 4u32,
		}));
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			foreign_asset: ETH_FLIP,
			batch_size: 4u32,
		}));

		assert_eq!(ScheduledEgress::<Test>::get(ETH_ETH), vec![]);
		assert_eq!(ScheduledEgress::<Test>::get(ETH_FLIP), vec![]);
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

		ScheduledEgress::<Test>::insert(
			ETH_FLIP,
			vec![
				(5_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(6_000, ForeignChainAddress::Eth(ALICE_ETH_ADDRESS)),
				(7_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
				(8_000, ForeignChainAddress::Eth(BOB_ETH_ADDRESS)),
			],
		);

		assert_eq!(LastEgressSent::get(), vec![]);

		assert_ok!(Egress::send_scheduled_egress_for_asset(Origin::root(), ETH_ETH));

		// Only `ETH_ETH` are egressed
		assert_eq!(
			LastEgressSent::get(),
			vec![
				([0xEE; 20], 1000u128, ALICE_ETH_ADDRESS),
				([0xEE; 20], 2000u128, ALICE_ETH_ADDRESS),
				([0xEE; 20], 3000u128, BOB_ETH_ADDRESS),
				([0xEE; 20], 4000u128, BOB_ETH_ADDRESS),
			]
		);
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			foreign_asset: ETH_ETH,
			batch_size: 4u32,
		}));

		assert!(ScheduledEgress::<Test>::get(ETH_ETH).is_empty());
		assert!(!ScheduledEgress::<Test>::get(ETH_FLIP).is_empty());
	});
}
