use crate::mock::*;

use cf_primitives::{ForeignChain, ForeignChainAddress, ForeignChainAsset};
use cf_traits::EgressApi;

use frame_support::{assert_noop, assert_ok};
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

// egress adds the transaction to storage
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

// on_idle sends the transactions out as batch

// transaction fees can be collected correctly
