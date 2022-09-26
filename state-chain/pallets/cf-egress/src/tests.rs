use crate::mock::*;

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
		assert!(crate::AllowedEgressAssets::<Test>::get(asset).is_none());
		assert_noop!(
			Egress::add_to_egress_batch(asset, 1_000, ForeignChainAddress::Eth(ALICE_ETH),),
			crate::Error::<Test>::AssetNotAllowedToEgress
		);

		// Enable the asset for Egress
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), asset, true));
		assert!(crate::AllowedEgressAssets::<Test>::get(asset).is_some());
		System::assert_last_event(Event::Egress(crate::Event::AssetPermissionSet {
			asset,
			allowed: true,
		}));
		assert_ok!(Egress::add_to_egress_batch(asset, 1_000, ForeignChainAddress::Eth(ALICE_ETH),));

		// Asset can be disabled
		// Enable the asset for Egress
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), asset, false));
		System::assert_last_event(Event::Egress(crate::Event::AssetPermissionSet {
			asset,
			allowed: false,
		}));
		assert!(crate::AllowedEgressAssets::<Test>::get(asset).is_none());
		assert_noop!(
			Egress::add_to_egress_batch(asset, 1_000, ForeignChainAddress::Eth(ALICE_ETH),),
			crate::Error::<Test>::AssetNotAllowedToEgress
		);
	});
}

#[test]
fn can_schedule_egress_to_batch() {
	new_test_ext().execute_with(|| {
		// Enable the asset for Egress
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_ETH, true));
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_FLIP, true));

		assert_ok!(Egress::add_to_egress_batch(
			ETH_ETH,
			1_000,
			ForeignChainAddress::Eth(ALICE_ETH),
		));
		assert_ok!(Egress::add_to_egress_batch(
			ETH_ETH,
			2_000,
			ForeignChainAddress::Eth(ALICE_ETH),
		));
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			asset: ETH_ETH,
			amount: 2_000,
			egress_address: ForeignChainAddress::Eth(ALICE_ETH),
		}));

		assert_ok!(
			Egress::add_to_egress_batch(ETH_FLIP, 3_000, ForeignChainAddress::Eth(BOB_ETH),)
		);
		assert_ok!(
			Egress::add_to_egress_batch(ETH_FLIP, 4_000, ForeignChainAddress::Eth(BOB_ETH),)
		);
		System::assert_last_event(Event::Egress(crate::Event::EgressScheduled {
			asset: ETH_FLIP,
			amount: 4_000,
			egress_address: ForeignChainAddress::Eth(BOB_ETH),
		}));

		assert_eq!(
			crate::ScheduledEgressBatches::<Test>::get(ETH_ETH),
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(2_000, ForeignChainAddress::Eth(ALICE_ETH))
			]
		);
		assert_eq!(
			crate::ScheduledEgressBatches::<Test>::get(ETH_FLIP),
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

		crate::ScheduledEgressBatches::<Test>::insert(
			ETH_ETH,
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(2_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(3_000, ForeignChainAddress::Eth(BOB_ETH)),
				(4_000, ForeignChainAddress::Eth(BOB_ETH)),
			],
		);

		crate::ScheduledEgressBatches::<Test>::insert(
			ETH_FLIP,
			vec![
				(5_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(6_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(7_000, ForeignChainAddress::Eth(BOB_ETH)),
				(8_000, ForeignChainAddress::Eth(BOB_ETH)),
			],
		);

		// Fee is set as 1000. Transaction sent is cleared
		assert_eq!(MockEthTrackedData::get().unwrap().base_fee, 1_000);
		assert_eq!(LastEgressSent::get(), vec![]);

		// Take all scheduled Egress and Broadcast as batch
		Egress::on_idle(1, 1_000_000_000_000u64);

		// ETH batch is sent first, followed by FLIP batch.
		assert_eq!(
			LastEgressSent::get(),
			vec![
				([0xFE; 20], 4750u128, ALICE_ETH),
				([0xFE; 20], 5750u128, ALICE_ETH),
				([0xFE; 20], 6750u128, BOB_ETH),
				([0xFE; 20], 7750u128, BOB_ETH),
			]
		);
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			asset: ETH_ETH,
			num_tx: 4u32,
			gas_fee: 1_000,
		}));
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			asset: ETH_FLIP,
			num_tx: 4u32,
			gas_fee: 1_000,
		}));

		assert_eq!(crate::ScheduledEgressBatches::<Test>::get(ETH_ETH), vec![]);
		assert_eq!(crate::ScheduledEgressBatches::<Test>::get(ETH_FLIP), vec![]);
	});
}

#[test]
fn fees_are_skimmed_from_txs() {
	new_test_ext().execute_with(|| {
		// Enable the asset for Egress
		assert_ok!(Egress::set_asset_egress_permission(Origin::root(), ETH_ETH, true));

		crate::ScheduledEgressBatches::<Test>::insert(
			ETH_ETH,
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
			],
		);

		// Fee is set as 1000. Transaction sent is cleared
		assert_eq!(MockEthTrackedData::get().unwrap().base_fee, 1_000);

		// Take all scheduled Egress and Broadcast as batch
		Egress::on_idle(1, 1_000_000_000_000u64);

		// 1000 is split evenly between the 4 transaction. -250 each
		assert_eq!(
			LastEgressSent::get(),
			vec![
				([0xFF; 20], 750u128, ALICE_ETH),
				([0xFF; 20], 750u128, ALICE_ETH),
				([0xFF; 20], 750u128, ALICE_ETH),
				([0xFF; 20], 750u128, ALICE_ETH),
			]
		);
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			asset: ETH_ETH,
			num_tx: 4u32,
			gas_fee: 1_000,
		}));
		assert_eq!(crate::ScheduledEgressBatches::<Test>::get(ETH_ETH), vec![]);

		// Change fee to 2000, split 5 ways
		crate::ScheduledEgressBatches::<Test>::insert(
			ETH_ETH,
			vec![
				(1_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(2_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(3_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(4_000, ForeignChainAddress::Eth(ALICE_ETH)),
				(5_000, ForeignChainAddress::Eth(ALICE_ETH)),
			],
		);

		// Fee is set as 1000. Transaction sent is cleared
		MockEthTrackedData::set(Some(TrackedData {
			block_height: 0,
			base_fee: 2_000,
			priority_fee: 5_000,
		}));

		// Take all scheduled Egress and Broadcast as batch
		Egress::on_idle(1, 1_000_000_000_000u64);

		assert_eq!(
			LastEgressSent::get(),
			vec![
				([0xFF; 20], 600u128, ALICE_ETH),
				([0xFF; 20], 1600u128, ALICE_ETH),
				([0xFF; 20], 2600u128, ALICE_ETH),
				([0xFF; 20], 3600u128, ALICE_ETH),
				([0xFF; 20], 4600u128, ALICE_ETH),
			]
		);
		System::assert_has_event(Event::Egress(crate::Event::EgressBroadcasted {
			asset: ETH_ETH,
			num_tx: 5u32,
			gas_fee: 2_000,
		}));
		assert_eq!(crate::ScheduledEgressBatches::<Test>::get(ETH_ETH), vec![]);
	});
}
