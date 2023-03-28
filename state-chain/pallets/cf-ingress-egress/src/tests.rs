use crate::{
	mock::*, AddressPool, AddressStatus, DeploymentStatus, DisabledEgressAssets, FetchOrTransfer,
	IntentAction, IntentActions, IntentExpiries, IntentIngressDetails, ScheduledEgressCcm,
	ScheduledEgressFetchOrTransfer, WeightInfo,
};

use cf_primitives::{
	chains::assets::eth, CcmIngressMetadata, ForeignChain, ForeignChainAddress, IntentId,
};
use cf_traits::{AddressDerivationApi, EgressApi, IngressApi};

use frame_support::{assert_ok, instances::Instance1, traits::Hooks, weights::Weight};
const ALICE_ETH_ADDRESS: EthereumAddress = [100u8; 20];
const BOB_ETH_ADDRESS: EthereumAddress = [101u8; 20];
const ETH_ETH: eth::Asset = eth::Asset::Eth;
const ETH_FLIP: eth::Asset = eth::Asset::Flip;
const EXPIRY_BLOCK: u64 = 6;

#[test]
fn disallowed_asset_will_not_be_batch_sent() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;

		// Cannot egress assets that are blacklisted.
		assert!(DisabledEgressAssets::<Test, Instance1>::get(asset).is_none());
		assert_ok!(IngressEgress::disable_asset_egress(RuntimeOrigin::root(), asset, true));
		assert!(DisabledEgressAssets::<Test, Instance1>::get(asset).is_some());
		System::assert_last_event(RuntimeEvent::IngressEgress(crate::Event::AssetEgressDisabled {
			asset,
			disabled: true,
		}));
		IngressEgress::schedule_egress(asset, 1_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::on_idle(1, Weight::from_ref_time(1_000_000_000_000u64));

		// The egress has not been sent
		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(),
			vec![FetchOrTransfer::<Ethereum>::Transfer {
				asset,
				amount: 1_000,
				egress_address: ALICE_ETH_ADDRESS.into(),
				egress_id: (ForeignChain::Ethereum, 1),
			}]
		);

		// re-enable the asset for Egress
		assert_ok!(IngressEgress::disable_asset_egress(RuntimeOrigin::root(), asset, false));
		assert!(DisabledEgressAssets::<Test, Instance1>::get(asset).is_none());
		System::assert_last_event(RuntimeEvent::IngressEgress(crate::Event::AssetEgressDisabled {
			asset,
			disabled: false,
		}));

		IngressEgress::on_idle(1, Weight::from_ref_time(1_000_000_000_000u64));

		// The egress should be sent now
		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());
	});
}

#[test]
fn can_schedule_swap_egress_to_batch() {
	new_test_ext().execute_with(|| {
		IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into(), None);
		System::assert_last_event(RuntimeEvent::IngressEgress(crate::Event::EgressScheduled {
			id: (ForeignChain::Ethereum, 2),
			asset: ETH_ETH,
			amount: 2_000,
			egress_address: ALICE_ETH_ADDRESS.into(),
		}));

		IngressEgress::schedule_egress(ETH_FLIP, 3_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 4_000, BOB_ETH_ADDRESS.into(), None);
		System::assert_last_event(RuntimeEvent::IngressEgress(crate::Event::EgressScheduled {
			id: (ForeignChain::Ethereum, 4),
			asset: ETH_FLIP,
			amount: 4_000,
			egress_address: BOB_ETH_ADDRESS.into(),
		}));

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_ETH,
					amount: 1_000,
					egress_address: ALICE_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 1),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_ETH,
					amount: 2_000,
					egress_address: ALICE_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 2),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 3_000,
					egress_address: BOB_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 3),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 4_000,
					egress_address: BOB_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 4),
				},
			]
		);
	});
}

fn schedule_ingress(who: u64, asset: eth::Asset) {
	let res = IngressEgress::register_liquidity_ingress_intent(who, asset);
	assert!(res.is_ok());

	if let Ok((_, ingress_address)) = res {
		assert_ok!(IngressEgress::do_single_ingress(
			ingress_address.try_into().unwrap(),
			asset,
			1_000,
			Default::default(),
		));
	}
}

#[test]
fn can_schedule_ingress_fetch() {
	new_test_ext().execute_with(|| {
		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());

		schedule_ingress(1u64, eth::Asset::Eth);
		schedule_ingress(2u64, eth::Asset::Eth);
		schedule_ingress(3u64, eth::Asset::Flip);

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 1u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 2u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 3u64, asset: ETH_FLIP },
			]
		);

		System::assert_has_event(RuntimeEvent::IngressEgress(
			crate::Event::IngressFetchesScheduled { intent_id: 1, asset: eth::Asset::Eth },
		));

		schedule_ingress(4u64, eth::Asset::Eth);

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 1u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 2u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 3u64, asset: ETH_FLIP },
				FetchOrTransfer::<Ethereum>::Fetch { intent_id: 4u64, asset: ETH_ETH },
			]
		);
	});
}

#[test]
fn on_idle_can_send_batch_all() {
	new_test_ext().execute_with(|| {
		IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS.into(), None);
		schedule_ingress(1u64, eth::Asset::Eth);
		schedule_ingress(2u64, eth::Asset::Eth);
		schedule_ingress(3u64, eth::Asset::Eth);
		schedule_ingress(4u64, eth::Asset::Eth);

		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS.into(), None);
		schedule_ingress(5u64, eth::Asset::Flip);

		// Take all scheduled Egress and Broadcast as batch
		IngressEgress::on_idle(1, Weight::from_ref_time(1_000_000_000_000u64));

		System::assert_has_event(RuntimeEvent::IngressEgress(
			crate::Event::BatchBroadcastRequested {
				broadcast_id: 1,
				egress_ids: vec![
					(ForeignChain::Ethereum, 1),
					(ForeignChain::Ethereum, 2),
					(ForeignChain::Ethereum, 3),
					(ForeignChain::Ethereum, 4),
					(ForeignChain::Ethereum, 5),
					(ForeignChain::Ethereum, 6),
					(ForeignChain::Ethereum, 7),
					(ForeignChain::Ethereum, 8),
				],
			},
		));

		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());
	});
}

#[test]
fn all_batch_apicall_creation_failure_should_rollback_storage() {
	new_test_ext().execute_with(|| {
		IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS.into(), None);
		schedule_ingress(1u64, eth::Asset::Eth);
		schedule_ingress(2u64, eth::Asset::Eth);
		schedule_ingress(3u64, eth::Asset::Eth);
		schedule_ingress(4u64, eth::Asset::Eth);

		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS.into(), None);
		schedule_ingress(5u64, eth::Asset::Flip);

		// This should create a failure since the environment of eth does not have any address
		// stored for USDC
		schedule_ingress(4u64, eth::Asset::Usdc);

		let scheduled_requests = ScheduledEgressFetchOrTransfer::<Test, Instance1>::get();

		// Try to send the scheduled egresses via Allbatch apicall. Will fail and so should rollback
		// the ScheduledEgressFetchOrTransfer
		IngressEgress::on_idle(1, Weight::from_ref_time(1_000_000_000_000u64));

		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(), scheduled_requests);
	});
}

#[test]
fn can_manually_send_batch_all() {
	new_test_ext().execute_with(|| {
		IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS.into(), None);
		schedule_ingress(1u64, eth::Asset::Eth);
		schedule_ingress(2u64, eth::Asset::Flip);
		IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS.into(), None);

		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS.into(), None);
		schedule_ingress(3u64, eth::Asset::Eth);
		schedule_ingress(4u64, eth::Asset::Flip);

		// Send only 2 requests
		assert_ok!(IngressEgress::egress_scheduled_fetch_transfer(RuntimeOrigin::root(), Some(2)));
		System::assert_has_event(RuntimeEvent::IngressEgress(
			crate::Event::BatchBroadcastRequested {
				broadcast_id: 1,
				egress_ids: vec![(ForeignChain::Ethereum, 1)],
			},
		));
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::decode_len(), Some(10));

		// send all remaining requests
		assert_ok!(IngressEgress::egress_scheduled_fetch_transfer(RuntimeOrigin::root(), None));

		System::assert_has_event(RuntimeEvent::IngressEgress(
			crate::Event::BatchBroadcastRequested {
				broadcast_id: 1,
				egress_ids: vec![
					(ForeignChain::Ethereum, 2),
					(ForeignChain::Ethereum, 3),
					(ForeignChain::Ethereum, 4),
					(ForeignChain::Ethereum, 5),
					(ForeignChain::Ethereum, 6),
					(ForeignChain::Ethereum, 7),
					(ForeignChain::Ethereum, 8),
				],
			},
		));

		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());
	});
}

#[test]
fn on_idle_batch_size_is_limited_by_weight() {
	new_test_ext().execute_with(|| {
		IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into(), None);
		schedule_ingress(1u64, eth::Asset::Eth);
		schedule_ingress(2u64, eth::Asset::Eth);
		IngressEgress::schedule_egress(ETH_FLIP, 3_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 4_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into(), None);
		schedule_ingress(3u64, eth::Asset::Flip);
		schedule_ingress(4u64, eth::Asset::Flip);

		// There's enough weights for 3 transactions, which are taken in FIFO order.
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::egress_assets(3) +
				Weight::from_ref_time(1),
		);

		System::assert_has_event(RuntimeEvent::IngressEgress(
			crate::Event::BatchBroadcastRequested {
				broadcast_id: 1,
				egress_ids: vec![(ForeignChain::Ethereum, 1), (ForeignChain::Ethereum, 2)],
			},
		));

		// Send another 3 requests.
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::egress_assets(3) +
				Weight::from_ref_time(1),
		);

		System::assert_has_event(RuntimeEvent::IngressEgress(
			crate::Event::BatchBroadcastRequested {
				broadcast_id: 1,
				egress_ids: vec![(ForeignChain::Ethereum, 3), (ForeignChain::Ethereum, 4)],
			},
		));

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 5_000,
					egress_address: ALICE_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 5),
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
			IngressEgress::on_idle(1, Weight::from_ref_time(1_000_000_000_000_000u64)),
			<Test as crate::Config<Instance1>>::WeightInfo::egress_assets(0) +
				<Test as crate::Config<Instance1>>::WeightInfo::egress_ccm(0)
		);

		// Blacklist Eth for Ethereum.
		let asset = ETH_ETH;
		assert_ok!(IngressEgress::disable_asset_egress(RuntimeOrigin::root(), asset, true));

		IngressEgress::schedule_egress(asset, 1_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(asset, 2_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(asset, 3_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(asset, 4_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(
			asset,
			1_000,
			ALICE_ETH_ADDRESS.into(),
			Some(CcmIngressMetadata {
				message: vec![],
				gas_budget: 0,
				refund_address: ForeignChainAddress::Eth(Default::default()),
			}),
		);
		assert_eq!(
			IngressEgress::on_idle(1, Weight::from_ref_time(1_000_000_000_000_000u64)),
			<Test as crate::Config<Instance1>>::WeightInfo::egress_assets(0) +
				<Test as crate::Config<Instance1>>::WeightInfo::egress_ccm(0)
		);

		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::decode_len(), Some(4));
		assert_eq!(ScheduledEgressCcm::<Test, Instance1>::decode_len(), Some(1));
	});
}

#[test]
fn intent_expires() {
	new_test_ext().execute_with(|| {
		let _ = IngressEgress::register_liquidity_ingress_intent(ALICE, ETH_ETH);
		assert!(AddressPool::<Test, Instance1>::get().is_empty());
		assert!(IntentExpiries::<Test, Instance1>::get(EXPIRY_BLOCK).is_some());
		let addresses =
			IntentExpiries::<Test, Instance1>::get(EXPIRY_BLOCK).expect("intent expiry exists");
		assert!(addresses.len() == 1);
		let address = addresses.get(0).expect("to have ingress details for that address").1;
		assert!(IntentIngressDetails::<Test, Instance1>::get(address,).is_some());
		assert!(IntentActions::<Test, Instance1>::get(address).is_some());
		AddressStatus::<Test, Instance1>::insert(address, DeploymentStatus::Deployed);
		IngressEgress::on_initialize(EXPIRY_BLOCK);
		assert!(IntentExpiries::<Test, Instance1>::get(EXPIRY_BLOCK).is_none());
		assert!(!AddressPool::<Test, Instance1>::get().is_empty());
		assert_eq!(
			AddressPool::<Test, Instance1>::get().pop().expect("to have a stale intent").1,
			address.clone()
		);
		System::assert_last_event(RuntimeEvent::IngressEgress(crate::Event::StopWitnessing {
			ingress_address: address,
			ingress_asset: ETH_ETH,
		}));
	});
}

#[test]
fn addresses_are_getting_reused() {
	new_test_ext().execute_with(|| {
		schedule_ingress(1u64, eth::Asset::Eth);
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::egress_assets(1) +
				Weight::from_ref_time(1),
		);
		IngressEgress::on_initialize(EXPIRY_BLOCK);
		assert!(!AddressPool::<Test, Instance1>::get().is_empty());
		assert_eq!(
			AddressStatus::<Test, Instance1>::get(
				AddressPool::<Test, Instance1>::get().pop().expect("to have an address").1
			),
			DeploymentStatus::Deployed
		);
		schedule_ingress(2u64, eth::Asset::Eth);
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::egress_assets(1) +
				Weight::from_ref_time(1),
		);
		IngressEgress::on_initialize(EXPIRY_BLOCK);
		assert_eq!(AddressPool::<Test, Instance1>::get().len(), 1);
	});
}

#[test]
fn create_new_address_while_pool_is_empty() {
	new_test_ext().execute_with(|| {
		schedule_ingress(1u64, eth::Asset::Eth);
		schedule_ingress(2u64, eth::Asset::Eth);
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::egress_assets(3) +
				Weight::from_ref_time(1),
		);
		IngressEgress::on_initialize(EXPIRY_BLOCK);
		assert_eq!(AddressPool::<Test, Instance1>::get().len(), 2);
		schedule_ingress(3u64, eth::Asset::Eth);
		assert_eq!(AddressPool::<Test, Instance1>::get().len(), 1);
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::egress_assets(2) +
				Weight::from_ref_time(1),
		);
		IngressEgress::on_initialize(EXPIRY_BLOCK);
		assert_eq!(AddressPool::<Test, Instance1>::get().len(), 2);
	});
}

#[test]
fn reused_address_intent_id_matches() {
	new_test_ext().execute_with(|| {
		const INTENT_ID: IntentId = 0;
		let eth_address =
			<<Test as crate::Config<Instance1>>::AddressDerivation as AddressDerivationApi<
				Ethereum,
			>>::generate_address(eth::Asset::Eth, INTENT_ID)
			.unwrap();
		AddressPool::<Test, _>::append((INTENT_ID, eth_address));

		let (reused_intent_id, reused_address) = IngressEgress::register_ingress_intent(
			eth::Asset::Eth,
			IntentAction::LiquidityProvision { lp_account: 0 },
		)
		.unwrap();

		// The reused details should be the same as before.
		assert_eq!(reused_intent_id, INTENT_ID);
		assert_eq!(eth_address, reused_address);
	});
}
