use crate::{
	mock::*, AddressPool, AddressStatus, ChannelAction, ChannelIdCounter, CrossChainMessage,
	DeploymentStatus, DisabledEgressAssets, Error, FetchOrTransfer, Pallet, ScheduledEgressCcm,
	ScheduledEgressFetchOrTransfer, WeightInfo,
};
use cf_chains::{ExecutexSwapAndCall, TransferAssetParams};
use cf_primitives::{chains::assets::eth, ChannelId, ForeignChain};
use cf_traits::{
	mocks::{
		api_call::{MockEthEnvironment, MockEthereumApiCall},
		ccm_handler::{CcmRequest, MockCcmHandler},
	},
	AddressDerivationApi, DepositApi, EgressApi,
};
use frame_support::{assert_noop, assert_ok, instances::Instance1, traits::Hooks, weights::Weight};
use sp_core::H160;

const ALICE_ETH_ADDRESS: EthereumAddress = [100u8; 20];
const BOB_ETH_ADDRESS: EthereumAddress = [101u8; 20];
const ETH_ETH: eth::Asset = eth::Asset::Eth;
const ETH_FLIP: eth::Asset = eth::Asset::Flip;
const EXPIRY_BLOCK: u64 = 6;

fn expect_size_of_address_pool(size: usize) {
	assert_eq!(
		AddressPool::<Test, Instance1>::iter_keys().into_iter().count(),
		size,
		"Address pool size is incorrect!"
	);
}

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
				destination_address: ALICE_ETH_ADDRESS.into(),
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
			destination_address: ALICE_ETH_ADDRESS.into(),
		}));

		IngressEgress::schedule_egress(ETH_FLIP, 3_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 4_000, BOB_ETH_ADDRESS.into(), None);
		System::assert_last_event(RuntimeEvent::IngressEgress(crate::Event::EgressScheduled {
			id: (ForeignChain::Ethereum, 4),
			asset: ETH_FLIP,
			amount: 4_000,
			destination_address: BOB_ETH_ADDRESS.into(),
		}));

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_ETH,
					amount: 1_000,
					destination_address: ALICE_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 1),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_ETH,
					amount: 2_000,
					destination_address: ALICE_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 2),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 3_000,
					destination_address: BOB_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 3),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 4_000,
					destination_address: BOB_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 4),
				},
			]
		);
	});
}

fn request_address_and_deposit(
	who: u64,
	asset: eth::Asset,
) -> (ChannelId, <Ethereum as Chain>::ChainAccount) {
	let (id, address) = IngressEgress::request_liquidity_deposit_address(who, asset).unwrap();
	let address: <Ethereum as Chain>::ChainAccount = address.try_into().unwrap();
	assert_ok!(IngressEgress::process_single_deposit(address, asset, 1_000, Default::default(),));
	(id, address)
}

#[test]
fn can_schedule_deposit_fetch() {
	new_test_ext().execute_with(|| {
		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());

		request_address_and_deposit(1u64, eth::Asset::Eth);
		request_address_and_deposit(2u64, eth::Asset::Eth);
		request_address_and_deposit(3u64, eth::Asset::Flip);

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Fetch { channel_id: 1u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { channel_id: 2u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { channel_id: 3u64, asset: ETH_FLIP },
			]
		);

		System::assert_has_event(RuntimeEvent::IngressEgress(
			crate::Event::DepositFetchesScheduled { channel_id: 1, asset: eth::Asset::Eth },
		));

		request_address_and_deposit(4u64, eth::Asset::Eth);

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Fetch { channel_id: 1u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { channel_id: 2u64, asset: ETH_ETH },
				FetchOrTransfer::<Ethereum>::Fetch { channel_id: 3u64, asset: ETH_FLIP },
				FetchOrTransfer::<Ethereum>::Fetch { channel_id: 4u64, asset: ETH_ETH },
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
		request_address_and_deposit(1u64, eth::Asset::Eth);
		request_address_and_deposit(2u64, eth::Asset::Eth);
		request_address_and_deposit(3u64, eth::Asset::Eth);
		request_address_and_deposit(4u64, eth::Asset::Eth);

		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS.into(), None);
		request_address_and_deposit(5u64, eth::Asset::Flip);

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
		request_address_and_deposit(1u64, eth::Asset::Eth);
		request_address_and_deposit(2u64, eth::Asset::Eth);
		request_address_and_deposit(3u64, eth::Asset::Eth);
		request_address_and_deposit(4u64, eth::Asset::Eth);

		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS.into(), None);
		request_address_and_deposit(5u64, eth::Asset::Flip);

		// This should create a failure since the environment of eth does not have any address
		// stored for USDC
		request_address_and_deposit(4u64, eth::Asset::Usdc);

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
		request_address_and_deposit(1u64, eth::Asset::Eth);
		request_address_and_deposit(2u64, eth::Asset::Flip);
		IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS.into(), None);

		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS.into(), None);
		request_address_and_deposit(3u64, eth::Asset::Eth);
		request_address_and_deposit(4u64, eth::Asset::Flip);

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
		request_address_and_deposit(1u64, eth::Asset::Eth);
		request_address_and_deposit(2u64, eth::Asset::Eth);
		IngressEgress::schedule_egress(ETH_FLIP, 3_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 4_000, ALICE_ETH_ADDRESS.into(), None);
		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS.into(), None);
		request_address_and_deposit(3u64, eth::Asset::Flip);
		request_address_and_deposit(4u64, eth::Asset::Flip);

		// There's enough weights for 3 transactions, which are taken in FIFO order.
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::destination_assets(3) +
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
			<Test as crate::Config<Instance1>>::WeightInfo::destination_assets(3) +
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
					destination_address: ALICE_ETH_ADDRESS.into(),
					egress_id: (ForeignChain::Ethereum, 5),
				},
				FetchOrTransfer::<Ethereum>::Fetch { channel_id: 3u64, asset: ETH_FLIP },
				FetchOrTransfer::<Ethereum>::Fetch { channel_id: 4u64, asset: ETH_FLIP },
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
			<Test as crate::Config<Instance1>>::WeightInfo::destination_assets(0) +
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
				source_address: ForeignChainAddress::Eth([0xcf; 20]),
			}),
		);
		assert_eq!(
			IngressEgress::on_idle(1, Weight::from_ref_time(1_000_000_000_000_000u64)),
			<Test as crate::Config<Instance1>>::WeightInfo::destination_assets(0) +
				<Test as crate::Config<Instance1>>::WeightInfo::egress_ccm(0)
		);

		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::decode_len(), Some(4));
		assert_eq!(ScheduledEgressCcm::<Test, Instance1>::decode_len(), Some(1));
	});
}

#[test]
fn addresses_are_getting_reused() {
	let mut ingress_to_finalise: (ChannelId, <Ethereum as Chain>::ChainAccount) =
		Default::default();

	new_test_ext()
		.execute_as_block(1, || {
			// Schedule 2 ingress requests but only complete one:
			ingress_to_finalise = request_address_and_deposit(0u64, eth::Asset::Eth);
			IngressEgress::request_liquidity_deposit_address(0u64, eth::Asset::Eth).unwrap();
			// Indicates we have already generated 2 addresses
			assert_eq!(ChannelIdCounter::<Test, Instance1>::get(), 2);
		})
		.execute_as_block(2, || {
			IngressEgress::close_ingress_channel(
				ingress_to_finalise.0,
				ingress_to_finalise.1,
				DeploymentStatus::Deployed,
			);
			expect_size_of_address_pool(1);
			// Address 1 is free to use and in the pool of available addresses
			assert!(AddressPool::<Test, Instance1>::get(1).is_some());
			// Address 2 not
			assert!(AddressPool::<Test, Instance1>::get(2).is_none());
		})
		.execute_as_block(EXPIRY_BLOCK, || {
			assert_eq!(
				AddressStatus::<Test, Instance1>::get(
					AddressPool::<Test, Instance1>::get(1).expect("to have an address")
				),
				DeploymentStatus::Deployed
			);
			expect_size_of_address_pool(1);
			// Schedule another ingress request
			ingress_to_finalise = request_address_and_deposit(0u64, eth::Asset::Eth);
			expect_size_of_address_pool(0);
		})
		.execute_as_block(EXPIRY_BLOCK + 1, || {
			IngressEgress::close_ingress_channel(
				ingress_to_finalise.0,
				ingress_to_finalise.1,
				DeploymentStatus::Deployed,
			);
			// Expect the address to be reused which is indicated by the counter not being
			// incremented
			assert_eq!(ChannelIdCounter::<Test, Instance1>::get(), 2);
			expect_size_of_address_pool(1);
		});
}

#[test]
fn proof_address_pool_integrity() {
	new_test_ext().execute_with(|| {
		let ingresses = (0..3)
			.map(|id| request_address_and_deposit(id, eth::Asset::Eth))
			.collect::<Vec<_>>();
		// All addresses in use
		expect_size_of_address_pool(0);
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::destination_assets(3) +
				Weight::from_ref_time(1),
		);
		for ingress in ingresses {
			IngressEgress::close_ingress_channel(ingress.0, ingress.1, DeploymentStatus::Deployed);
		}
		// Expect all addresses to be available
		expect_size_of_address_pool(3);
		request_address_and_deposit(4u64, eth::Asset::Eth);
		// Expect one address to be in use
		expect_size_of_address_pool(2);
	});
}

#[test]
fn create_new_address_while_pool_is_empty() {
	new_test_ext().execute_with(|| {
		let ingresses = (0..2)
			.map(|id| request_address_and_deposit(id, eth::Asset::Eth))
			.collect::<Vec<_>>();
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::destination_assets(3) +
				Weight::from_ref_time(1),
		);
		for ingress in ingresses {
			IngressEgress::close_ingress_channel(ingress.0, ingress.1, DeploymentStatus::Deployed);
		}
		IngressEgress::on_initialize(EXPIRY_BLOCK);
		assert_eq!(ChannelIdCounter::<Test, Instance1>::get(), 2);
		request_address_and_deposit(3u64, eth::Asset::Eth);
		assert_eq!(ChannelIdCounter::<Test, Instance1>::get(), 2);
		IngressEgress::on_idle(
			1,
			<Test as crate::Config<Instance1>>::WeightInfo::destination_assets(2) +
				Weight::from_ref_time(1),
		);
		IngressEgress::on_initialize(EXPIRY_BLOCK);
		assert_eq!(ChannelIdCounter::<Test, Instance1>::get(), 2);
	});
}

#[test]
fn reused_address_channel_id_matches() {
	new_test_ext().execute_with(|| {
		const INTENT_ID: ChannelId = 0;
		let eth_address =
			<<Test as crate::Config<Instance1>>::AddressDerivation as AddressDerivationApi<
				Ethereum,
			>>::generate_address(eth::Asset::Eth, INTENT_ID)
			.unwrap();
		AddressPool::<Test, _>::insert(INTENT_ID, eth_address);

		let (reused_channel_id, reused_address) = IngressEgress::open_channel(
			eth::Asset::Eth,
			ChannelAction::LiquidityProvision { lp_account: 0 },
		)
		.unwrap();

		// The reused details should be the same as before.
		assert_eq!(reused_channel_id, INTENT_ID);
		assert_eq!(eth_address, reused_address);
	});
}

#[test]
fn can_process_ccm_deposit() {
	new_test_ext().execute_with(|| {
		let from_asset = eth::Asset::Flip;
		let to_asset = Asset::Eth;
		let destination_address = ForeignChainAddress::Eth(Default::default());
		let ccm = CcmIngressMetadata {
			message: vec![0x00, 0x01, 0x02],
			gas_budget: 1_000,
			refund_address: ForeignChainAddress::Eth(Default::default()),
			source_address: ForeignChainAddress::Eth([0xcf; 20]),
		};
		let amount = 5_000;

		// Register swap deposit with CCM
		assert_ok!(IngressEgress::request_swap_deposit_address(
			from_asset,
			to_asset,
			destination_address.clone(),
			0,
			1,
			Some(ccm.clone()),
		));

		// CCM action is stored.
		let deposit_address = hex_literal::hex!("bc77955482380836042253381b35a658d87a4842").into();
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, Instance1>::StartWitnessing {
				deposit_address,
				source_asset: from_asset,
			},
		));

		// Completing the ingress should trigger CcmHandler.
		assert_ok!(IngressEgress::process_single_deposit(
			deposit_address,
			from_asset,
			amount,
			Default::default(),
		));
		assert_eq!(
			MockCcmHandler::get_ccm_requests(),
			vec![CcmRequest {
				source_asset: from_asset.into(),
				deposit_amount: amount,
				destination_asset: to_asset,
				destination_address,
				message_metadata: ccm
			}]
		);
	});
}

#[test]
fn can_egress_ccm() {
	new_test_ext().execute_with(|| {
		let destination_address: H160 = [0x01; 20].into();
		let destination_asset = eth::Asset::Eth;
		let ccm = CcmIngressMetadata {
			message: vec![0x00, 0x01, 0x02],
			gas_budget: 1_000,
			refund_address: ForeignChainAddress::Eth([0x02; 20]),
			source_address: ForeignChainAddress::Eth([0xcf; 20]),
		};
		let amount = 5_000;
		let egress_id = IngressEgress::schedule_egress(
			destination_asset,
			amount,
			destination_address,
			Some(ccm.clone())
		);

		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());
		assert_eq!(ScheduledEgressCcm::<Test, Instance1>::get(), vec![
			CrossChainMessage {
				egress_id,
				asset: destination_asset,
				amount,
				destination_address,
				message: ccm.message.clone(),
				refund_address: ForeignChainAddress::Eth([0x02; 20]),
				source_address: ForeignChainAddress::Eth([0xcf; 20]),
			}
		]);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, Instance1>::EgressScheduled {
				id: egress_id,
				asset: destination_asset,
				amount,
				destination_address,
			}
		));

		// Send the scheduled ccm in on_idle
		IngressEgress::on_idle(1, Weight::from_ref_time(1_000_000_000_000u64));

		// Check that the CCM should be egressed
		assert_eq!(EgressedApiCalls::get(), vec![<MockEthereumApiCall<MockEthEnvironment> as ExecutexSwapAndCall<Ethereum>>::new_unsigned(
			(ForeignChain::Ethereum, 1),
			TransferAssetParams {
				asset: destination_asset,
				amount,
				to: destination_address
			},
			ForeignChainAddress::Eth([0xcf; 20]),
			ccm.message,
		).unwrap()]);

		// Storage should be cleared
		assert_eq!(ScheduledEgressCcm::<Test, Instance1>::decode_len(), Some(0));
	});
}

#[test]
fn can_manually_egress_ccm() {
	new_test_ext().execute_with(|| {
		let destination_address: H160 = [0x01; 20].into();
		let destination_asset = eth::Asset::Eth;
		let message = vec![0x00, 0x01, 0x02];
		let amount = 5_000;

		ScheduledEgressCcm::<Test, Instance1>::append(
			CrossChainMessage {
				egress_id: (ForeignChain::Ethereum, 1),
				asset: destination_asset,
				amount,
				destination_address,
				message: message.clone(),
				refund_address: ForeignChainAddress::Eth([0x02; 20]),
				source_address: ForeignChainAddress::Eth([0xcf; 20]),
			}
		);

		// Governance can scheduled ccm egress
		assert_ok!(IngressEgress::egress_scheduled_ccms(
			RuntimeOrigin::root(),
			None,
		));

		// Check that the CCM should be egressed
		assert_eq!(EgressedApiCalls::get(), vec![<MockEthereumApiCall<MockEthEnvironment> as ExecutexSwapAndCall<Ethereum>>::new_unsigned(
			(ForeignChain::Ethereum, 1),
			TransferAssetParams {
				asset: destination_asset,
				amount,
				to: destination_address
			},
			ForeignChainAddress::Eth([0xcf; 20]),
			message,
		).unwrap()]);

		// Storage should be cleared
		assert_eq!(ScheduledEgressCcm::<Test, Instance1>::decode_len(), Some(0));
	});
}

#[test]
fn can_manually_egress_ccm_by_id() {
	new_test_ext().execute_with(|| {
		let destination_address: H160 = [0x01; 20].into();
		let destination_asset = eth::Asset::Eth;
		let message = vec![0x00, 0x01, 0x02];
		let amount = 5_000;

		// Helper function that creates a CrossChainMessage using a given ID.
		let new_ccm = |id: u64| -> CrossChainMessage<Ethereum> {
			CrossChainMessage {
				egress_id: (ForeignChain::Ethereum, id),
				asset: destination_asset,
				amount,
				destination_address,
				message: message.clone(),
				refund_address: ForeignChainAddress::Eth([0x02; 20]),
				source_address: ForeignChainAddress::Eth([0xcf; 20]),
			}
		};
		// Helper function that constructs a ExecutexSwapAndCall ApiCall from a given
		// CrossChainMessage.
		let to_api_call =
			|ccm: CrossChainMessage<Ethereum>| -> MockEthereumApiCall<MockEthEnvironment> {
				<MockEthereumApiCall<MockEthEnvironment> as ExecutexSwapAndCall<Ethereum>>::new_unsigned(
				ccm.egress_id,
				TransferAssetParams {
					asset: ccm.asset,
					amount: ccm.amount,
					to: ccm.destination_address
				},
				ccm.source_address,
				ccm.message,
			).unwrap()
			};
		// Helper function that creates a FetchOrTransfer::Transfer using a given ID.
		let transfer = FetchOrTransfer::Transfer {
			egress_id: (ForeignChain::Ethereum, 4),
			asset: destination_asset,
			destination_address,
			amount,
		};

		ScheduledEgressCcm::<Test, Instance1>::set(vec![new_ccm(1), new_ccm(2), new_ccm(3)]);
		ScheduledEgressFetchOrTransfer::<Test, Instance1>::set(vec![transfer.clone()]);

		// send scheduled ccm egress by ID
		assert_ok!(IngressEgress::egress_scheduled_ccms_by_egress_id(
			RuntimeOrigin::root(),
			vec![
				(ForeignChain::Ethereum, 1),
				(ForeignChain::Ethereum, 3),
				// Should only affect Ccm, not FetchOrTransfer
				(ForeignChain::Ethereum, 4)
			],
		));

		// Check that the CCMs and only CCMs are egressed
		assert_eq!(
			EgressedApiCalls::get(),
			vec![to_api_call(new_ccm(1)), to_api_call(new_ccm(3)),]
		);
		// Egressed ccms are cleared from storage.
		assert_eq!(ScheduledEgressCcm::<Test, Instance1>::get(), vec![new_ccm(2)]);

		// FetchOrTransfer should not be affected
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get(), vec![transfer]);
	});
}
#[test]
fn multi_use_ingress_different_blocks() {
	const ETH: eth::Asset = eth::Asset::Eth;
	let (mut channel_id, mut deposit_address): (ChannelId, <Ethereum as Chain>::ChainAccount) =
		Default::default();

	new_test_ext()
		.execute_as_block(1, || {
			(channel_id, deposit_address) = request_address_and_deposit(ALICE, ETH);
		})
		.execute_as_block(2, || {
			// Do another, should succeed.
			assert_ok!(Pallet::<Test, _>::process_single_deposit(
				deposit_address,
				ETH,
				1,
				Default::default()
			));
		})
		.execute_as_block(3, || {
			// Finalising should invalidate the ingress.
			IngressEgress::close_ingress_channel(
				channel_id,
				deposit_address,
				DeploymentStatus::Deployed,
			);
			assert_noop!(
				Pallet::<Test, _>::process_single_deposit(
					deposit_address,
					ETH,
					1,
					Default::default()
				),
				Error::<Test, _>::InvalidIntent
			);
		});
}

#[test]
fn multi_use_ingress_same_block() {
	const ETH: eth::Asset = eth::Asset::Eth;
	new_test_ext()
		.execute_as_block(1, || {
			let (_channel_id, deposit_address) = request_address_and_deposit(ALICE, ETH);
			// Another ingress to the same address.
			Pallet::<Test, _>::process_single_deposit(deposit_address, ETH, 1, Default::default())
				.unwrap();
		})
		.execute_with(|| {
			assert_eq!(
				ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(),
				0
			);
		});
}
