use crate::{mock::*, AllowedEgressAssets, ScheduledEgress};

use cf_primitives::{ForeignChain, ForeignChainAddress, ForeignChainAsset};
use cf_traits::EgressApi;

use frame_support::{assert_noop, assert_ok, traits::Hooks};
const ALICE_ETH: EthereumAddress = [100u8; 20];
const BOB_ETH: EthereumAddress = [101u8; 20];
const ETH_ETH: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth };
const ETH_FLIP: ForeignChainAsset =
	ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Flip };

#[test]
fn can_only_egress_allowed_asset() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;

		// Cannot egress assets that are not whitelisted.
		assert!(AllowedEgressAssets::<Test>::get(asset).is_none());
		assert_noop!(
			Egress::egress_asset(asset, 1_000, ForeignChainAddress::Eth(ALICE_ETH),),
			crate::Error::<Test>::AssetEgressDisallowed
		);

		// Enable the asset for Egress
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), asset, true));
		assert!(AllowedEgressAssets::<Test>::get(asset).is_some());
		System::assert_last_event(Event::Egress(crate::Event::AssetPermissionSet {
			asset,
			allowed: true,
		}));
		assert_ok!(Egress::egress_asset(asset, 1_000, ForeignChainAddress::Eth(ALICE_ETH),));

		// Asset can be disabled
		// Enable the asset for Egress
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), asset, false));
		System::assert_last_event(Event::Egress(crate::Event::AssetPermissionSet {
			asset,
			allowed: false,
		}));
		assert!(AllowedEgressAssets::<Test>::get(asset).is_none());
		assert_noop!(
			Egress::egress_asset(asset, 1_000, ForeignChainAddress::Eth(ALICE_ETH),),
			crate::Error::<Test>::AssetEgressDisallowed
		);
	});
}

#[test]
fn can_schedule_egress_to_batch() {
	new_test_ext().execute_with(|| {
		// Enable the asset for Egress
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_ETH, true));
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_FLIP, true));

		assert_ok!(Egress::egress_asset(ETH_ETH, 1_000, ForeignChainAddress::Eth(ALICE_ETH),));
		assert_ok!(Egress::egress_asset(ETH_ETH, 2_000, ForeignChainAddress::Eth(ALICE_ETH),));
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			foreign_asset: ETH_ETH,
			amount: 2_000,
			egress_address: ForeignChainAddress::Eth(ALICE_ETH),
		}));

		assert_ok!(Egress::egress_asset(ETH_FLIP, 3_000, ForeignChainAddress::Eth(BOB_ETH),));
		assert_ok!(Egress::egress_asset(ETH_FLIP, 4_000, ForeignChainAddress::Eth(BOB_ETH),));
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			foreign_asset: ETH_FLIP,
			amount: 4_000,
			egress_address: ForeignChainAddress::Eth(BOB_ETH),
		}));

		assert_eq!(
			ScheduledEgress::<Test>::get(ETH_ETH),
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(2_000, ForeignChainAddress::Eth(ALICE_ETH))
			]
		);
		assert_eq!(
			ScheduledEgress::<Test>::get(ETH_FLIP),
			vec![
				(3_000, ForeignChainAddress::Eth(BOB_ETH)),
				(4_000, ForeignChainAddress::Eth(BOB_ETH))
			]
		);
	});
}

#[test]
fn on_idle_can_send_batch_all() {
	new_test_ext().execute_with(|| {
		// Enable the asset for Egress
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_ETH, true));
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_FLIP, true));

		ScheduledEgress::<Test>::insert(
			ETH_ETH,
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(2_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(3_000, ForeignChainAddress::Eth(BOB_ETH)),
				(4_000, ForeignChainAddress::Eth(BOB_ETH)),
			],
		);

		ScheduledEgress::<Test>::insert(
			ETH_FLIP,
			vec![
				(5_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(6_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(7_000, ForeignChainAddress::Eth(BOB_ETH)),
				(8_000, ForeignChainAddress::Eth(BOB_ETH)),
			],
		);

		assert_eq!(LastEgressSent::get(), vec![]);

		// Take all scheduled Egress and Broadcast as batch
		Egress::on_idle(1, 1_000_000_000_000u64);

		// ETH batch is sent first, followed by FLIP batch.
		assert_eq!(
			LastEgressSent::get(),
			vec![
				([0x00; 20], 5000u128, ALICE_ETH),
				([0x00; 20], 6000u128, ALICE_ETH),
				([0x00; 20], 7000u128, BOB_ETH),
				([0x00; 20], 8000u128, BOB_ETH),
			]
		);
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			asset: ETH_ETH,
			batch_size: 4u32,
		}));
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			asset: ETH_FLIP,
			batch_size: 4u32,
		}));

		assert_eq!(ScheduledEgress::<Test>::get(ETH_ETH), vec![]);
		assert_eq!(ScheduledEgress::<Test>::get(ETH_FLIP), vec![]);
	});
}

#[test]
fn egress_chain_and_asset_must_match() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;

		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_ETH, true));
		assert_noop!(
			Egress::egress_asset(asset, 1_000, ForeignChainAddress::Dot([0x00; 32])),
			crate::Error::<Test>::InvalidEgressDestination
		);
	});
}

#[test]
fn governance_can_send_batch_all() {
	new_test_ext().execute_with(|| {
		// Enable the asset for Egress
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_ETH, true));
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_FLIP, true));

		ScheduledEgress::<Test>::insert(
			ETH_ETH,
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(2_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(3_000, ForeignChainAddress::Eth(BOB_ETH)),
				(4_000, ForeignChainAddress::Eth(BOB_ETH)),
			],
		);

		ScheduledEgress::<Test>::insert(
			ETH_FLIP,
			vec![
				(5_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(6_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(7_000, ForeignChainAddress::Eth(BOB_ETH)),
				(8_000, ForeignChainAddress::Eth(BOB_ETH)),
			],
		);

		assert_eq!(LastEgressSent::get(), vec![]);

		// Take all scheduled Egress and Broadcast as batch
		assert_ok!(Egress::send_scheduled_egress_for_asset(Origin::root(), ETH_ETH));

		// Only `ETH_ETH` are egressed
		assert_eq!(
			LastEgressSent::get(),
			vec![
				([0xFF; 20], 1000u128, ALICE_ETH),
				([0xFF; 20], 2000u128, ALICE_ETH),
				([0xFF; 20], 3000u128, BOB_ETH),
				([0xFF; 20], 4000u128, BOB_ETH),
			]
		);
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			asset: ETH_ETH,
			batch_size: 4u32,
		}));

		assert!(ScheduledEgress::<Test>::get(ETH_ETH).is_empty());
		assert!(!ScheduledEgress::<Test>::get(ETH_FLIP).is_empty());
	});
}
