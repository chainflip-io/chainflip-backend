use crate::{
	mock::*, Call as PalletCall, ChannelAction, ChannelIdCounter, CrossChainMessage,
	DepositChannelLookup, DepositChannelPool, DepositWitness, DisabledEgressAssets, Error,
	Event as PalletEvent, FetchOrTransfer, MinimumDeposit, Pallet, ScheduledEgressCcm,
	ScheduledEgressFetchOrTransfer,
};
use cf_chains::{
	address::AddressConverter, eth::EthereumFetchId, CcmChannelMetadata, DepositChannel,
	ExecutexSwapAndCall, SwapOrigin, TransferAssetParams,
};
use cf_primitives::{chains::assets::eth, ChannelId, ForeignChain};
use cf_test_utilities::assert_has_event;
use cf_traits::{
	mocks::{
		address_converter::MockAddressConverter,
		api_call::{MockAllBatch, MockEthEnvironment, MockEthereumApiCall},
		ccm_handler::{CcmRequest, MockCcmHandler},
	},
	DepositApi, EgressApi, GetBlockHeight,
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{Hooks, OriginTrait},
};
use sp_core::H160;

const ALICE_ETH_ADDRESS: EthereumAddress = H160([100u8; 20]);
const BOB_ETH_ADDRESS: EthereumAddress = H160([101u8; 20]);
const ETH_ETH: eth::Asset = eth::Asset::Eth;
const ETH_FLIP: eth::Asset = eth::Asset::Flip;
const EXPIRY_BLOCK: u64 = 6;

#[track_caller]
fn expect_size_of_address_pool(size: usize) {
	assert_eq!(
		DepositChannelPool::<Test>::iter_keys().count(),
		size,
		"Address pool size is incorrect!"
	);
}

#[test]
fn blacklisted_asset_will_not_egress_via_batch_all() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;

		// Cannot egress assets that are blacklisted.
		assert!(DisabledEgressAssets::<Test>::get(asset).is_none());
		assert_ok!(IngressEgress::enable_or_disable_egress(RuntimeOrigin::root(), asset, true));
		assert!(DisabledEgressAssets::<Test>::get(asset).is_some());
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::AssetEgressStatusChanged { asset, disabled: true },
		));

		// Eth should be blocked while Flip can be sent
		IngressEgress::schedule_egress(asset, 1_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_FLIP, 1_000, ALICE_ETH_ADDRESS, None);

		IngressEgress::on_finalize(1);

		// The egress has not been sent
		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test>::get(),
			vec![FetchOrTransfer::<Ethereum>::Transfer {
				asset,
				amount: 1_000,
				destination_address: ALICE_ETH_ADDRESS,
				egress_id: (ForeignChain::Ethereum, 1),
			}]
		);

		// re-enable the asset for Egress
		assert_ok!(IngressEgress::enable_or_disable_egress(RuntimeOrigin::root(), asset, false));
		assert!(DisabledEgressAssets::<Test>::get(asset).is_none());
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::AssetEgressStatusChanged { asset, disabled: false },
		));

		IngressEgress::on_finalize(1);

		// The egress should be sent now
		assert!(ScheduledEgressFetchOrTransfer::<Test>::get().is_empty());
	});
}

#[test]
fn blacklisted_asset_will_not_egress_via_ccm() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;
		let ccm = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
			channel_metadata: CcmChannelMetadata {
				message: vec![0x00, 0x01, 0x02],
				gas_budget: 1_000,
				cf_parameters: vec![],
			},
		};

		assert!(DisabledEgressAssets::<Test>::get(asset).is_none());
		assert_ok!(IngressEgress::enable_or_disable_egress(RuntimeOrigin::root(), asset, true));

		// Eth should be blocked while Flip can be sent
		IngressEgress::schedule_egress(asset, 1_000, ALICE_ETH_ADDRESS, Some(ccm.clone()));
		IngressEgress::schedule_egress(ETH_FLIP, 1_000, ALICE_ETH_ADDRESS, Some(ccm.clone()));

		IngressEgress::on_finalize(1);

		// The egress has not been sent
		assert_eq!(
			ScheduledEgressCcm::<Test>::get(),
			vec![CrossChainMessage {
				egress_id: (ForeignChain::Ethereum, 1),
				asset,
				amount: 1_000,
				destination_address: ALICE_ETH_ADDRESS,
				message: ccm.channel_metadata.message.clone(),
				source_chain: ForeignChain::Ethereum,
				source_address: ccm.source_address.clone(),
				cf_parameters: ccm.channel_metadata.cf_parameters,
			}]
		);

		// re-enable the asset for Egress
		assert_ok!(IngressEgress::enable_or_disable_egress(RuntimeOrigin::root(), asset, false));

		IngressEgress::on_finalize(2);

		// The egress should be sent now
		assert!(ScheduledEgressCcm::<Test>::get().is_empty());
	});
}

#[test]
fn can_schedule_swap_egress_to_batch() {
	new_test_ext().execute_with(|| {
		IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS, None);
		System::assert_last_event(RuntimeEvent::IngressEgress(crate::Event::EgressScheduled {
			id: (ForeignChain::Ethereum, 2),
			asset: ETH_ETH,
			amount: 2_000,
			destination_address: ALICE_ETH_ADDRESS,
		}));

		IngressEgress::schedule_egress(ETH_FLIP, 3_000, BOB_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_FLIP, 4_000, BOB_ETH_ADDRESS, None);
		System::assert_last_event(RuntimeEvent::IngressEgress(crate::Event::EgressScheduled {
			id: (ForeignChain::Ethereum, 4),
			asset: ETH_FLIP,
			amount: 4_000,
			destination_address: BOB_ETH_ADDRESS,
		}));

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test>::get(),
			vec![
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_ETH,
					amount: 1_000,
					destination_address: ALICE_ETH_ADDRESS,
					egress_id: (ForeignChain::Ethereum, 1),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_ETH,
					amount: 2_000,
					destination_address: ALICE_ETH_ADDRESS,
					egress_id: (ForeignChain::Ethereum, 2),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 3_000,
					destination_address: BOB_ETH_ADDRESS,
					egress_id: (ForeignChain::Ethereum, 3),
				},
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_FLIP,
					amount: 4_000,
					destination_address: BOB_ETH_ADDRESS,
					egress_id: (ForeignChain::Ethereum, 4),
				},
			]
		);
	});
}

fn request_address_and_deposit(
	who: ChannelId,
	asset: eth::Asset,
) -> (ChannelId, <Ethereum as Chain>::ChainAccount) {
	let (id, address) = IngressEgress::request_liquidity_deposit_address(who, asset).unwrap();
	let address: <Ethereum as Chain>::ChainAccount = address.try_into().unwrap();
	assert_ok!(IngressEgress::process_single_deposit(address, asset, 1_000, ()));
	(id, address)
}

#[test]
fn can_schedule_deposit_fetch() {
	new_test_ext().execute_with(|| {
		assert!(ScheduledEgressFetchOrTransfer::<Test>::get().is_empty());

		request_address_and_deposit(1u64, eth::Asset::Eth);
		request_address_and_deposit(2u64, eth::Asset::Eth);
		request_address_and_deposit(3u64, eth::Asset::Flip);

		assert!(matches!(
			&ScheduledEgressFetchOrTransfer::<Test>::get()[..],
			&[
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_ETH, .. },
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_ETH, .. },
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_FLIP, .. },
			]
		));

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(
			crate::Event::DepositFetchesScheduled { channel_id: 1, asset: eth::Asset::Eth },
		));

		request_address_and_deposit(4u64, eth::Asset::Eth);

		assert!(matches!(
			&ScheduledEgressFetchOrTransfer::<Test>::get()[..],
			&[
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_ETH, .. },
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_ETH, .. },
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_FLIP, .. },
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_ETH, .. },
			]
		));
	});
}

#[test]
fn on_finalize_can_send_batch_all() {
	new_test_ext().execute_with(|| {
		IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS, None);
		request_address_and_deposit(1u64, eth::Asset::Eth);
		request_address_and_deposit(2u64, eth::Asset::Eth);
		request_address_and_deposit(3u64, eth::Asset::Eth);
		request_address_and_deposit(4u64, eth::Asset::Eth);

		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS, None);
		request_address_and_deposit(5u64, eth::Asset::Flip);

		// Take all scheduled Egress and Broadcast as batch
		IngressEgress::on_finalize(1);

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(
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

		assert!(ScheduledEgressFetchOrTransfer::<Test>::get().is_empty());
	});
}

#[test]
fn all_batch_apicall_creation_failure_should_rollback_storage() {
	new_test_ext().execute_with(|| {
		IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS, None);
		request_address_and_deposit(1u64, eth::Asset::Eth);
		request_address_and_deposit(2u64, eth::Asset::Eth);
		request_address_and_deposit(3u64, eth::Asset::Eth);
		request_address_and_deposit(4u64, eth::Asset::Eth);

		IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS, None);
		IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS, None);
		request_address_and_deposit(5u64, eth::Asset::Flip);

		// This should create a failure since the environment of eth does not have any address
		// stored for USDC
		request_address_and_deposit(4u64, eth::Asset::Usdc);

		let scheduled_requests = ScheduledEgressFetchOrTransfer::<Test>::get();

		// Try to send the scheduled egresses via Allbatch apicall. Will fail and so should rollback
		// the ScheduledEgressFetchOrTransfer
		IngressEgress::on_finalize(1);

		assert_eq!(ScheduledEgressFetchOrTransfer::<Test>::get(), scheduled_requests);
	});
}

#[test]
fn addresses_are_getting_reused() {
	new_test_ext()
		// Request 2 deposit addresses and deposit to one of them.
		.request_address_and_deposit(&[
			(ALICE, eth::Asset::Eth, 100u32.into()),
			(ALICE, eth::Asset::Eth, 0u32.into()),
		])
		.inspect_storage(|deposit_details| {
			assert_eq!(ChannelIdCounter::<Test, _>::get(), deposit_details.len() as u64);
		})
		// Simulate broadcast success.
		.then_process_events(|_ctx, event| match event {
			RuntimeEvent::IngressEgress(PalletEvent::BatchBroadcastRequested {
				broadcast_id,
				..
			}) => Some(broadcast_id),
			_ => None,
		})
		.then_execute_at_next_block(|(channels, broadcast_ids)| {
			assert!(broadcast_ids.len() == 1);
			// This would normally be triggered on broadcast success, should finalise the ingress.
			for id in broadcast_ids {
				MockEgressBroadcaster::dispatch_callback(id);
			}
			channels
		})
		// Close the channels.
		.then_execute_at_next_block(|channels| {
			for (_id, address, _asset) in &channels {
				IngressEgress::close_channel(*address);
			}
			channels[0]
		})
		// Check that the used address is now deployed and in the pool of available addresses.
		.inspect_storage(|(channel_id, address, _asset)| {
			expect_size_of_address_pool(1);
			// Address 1 is free to use and in the pool of available addresses
			assert_eq!(DepositChannelPool::<Test, _>::get(channel_id).unwrap().address, *address);
		})
		.request_deposit_addresses(&[(ALICE, eth::Asset::Eth)])
		// The address should have been taken from the pool and the id counter unchanged.
		.inspect_storage(|_| {
			expect_size_of_address_pool(0);
			assert_eq!(ChannelIdCounter::<Test, _>::get(), 2);
		});
}

#[test]
fn proof_address_pool_integrity() {
	new_test_ext().execute_with(|| {
		let channel_details = (0..3)
			.map(|id| request_address_and_deposit(id, eth::Asset::Eth))
			.collect::<Vec<_>>();
		// All addresses in use
		expect_size_of_address_pool(0);
		IngressEgress::on_finalize(1);
		for (_id, address) in channel_details {
			assert_ok!(IngressEgress::finalise_ingress(RuntimeOrigin::root(), vec![address]));
			IngressEgress::close_channel(address);
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
		let channel_details = (0..2)
			.map(|id| request_address_and_deposit(id, eth::Asset::Eth))
			.collect::<Vec<_>>();
		IngressEgress::on_finalize(1);
		for (_id, address) in channel_details {
			assert_ok!(IngressEgress::finalise_ingress(RuntimeOrigin::root(), vec![address]));
			IngressEgress::close_channel(address);
		}
		IngressEgress::on_initialize(EXPIRY_BLOCK);
		assert_eq!(ChannelIdCounter::<Test>::get(), 2);
		request_address_and_deposit(3u64, eth::Asset::Eth);
		assert_eq!(ChannelIdCounter::<Test>::get(), 2);
		IngressEgress::on_finalize(1);
		IngressEgress::on_initialize(EXPIRY_BLOCK);
		assert_eq!(ChannelIdCounter::<Test>::get(), 2);
	});
}

#[test]
fn reused_address_channel_id_matches() {
	new_test_ext().execute_with(|| {
		const CHANNEL_ID: ChannelId = 0;
		let new_channel = DepositChannel::<Ethereum>::generate_new::<
			<Test as crate::Config>::AddressDerivation,
		>(CHANNEL_ID, eth::Asset::Eth)
		.unwrap();
		DepositChannelPool::<Test, _>::insert(CHANNEL_ID, new_channel.clone());
		let (reused_channel_id, reused_address) = IngressEgress::open_channel(
			eth::Asset::Eth,
			ChannelAction::LiquidityProvision { lp_account: 0 },
		)
		.unwrap();
		// The reused details should be the same as before.
		assert_eq!(new_channel.channel_id, reused_channel_id);
		assert_eq!(new_channel.address, reused_address);
	});
}

#[test]
fn can_process_ccm_deposit() {
	new_test_ext().execute_with(|| {
		let from_asset = eth::Asset::Flip;
		let to_asset = Asset::Eth;
		let destination_address = ForeignChainAddress::Eth(Default::default());
		let channel_metadata = CcmChannelMetadata {
			message: vec![0x00, 0x01, 0x02],
			gas_budget: 1_000,
			cf_parameters: vec![],
		};
		let ccm = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: None,
			channel_metadata: channel_metadata.clone(),
		};
		let amount = 5_000;

		// Register swap deposit with CCM
		assert_ok!(IngressEgress::request_swap_deposit_address(
			from_asset,
			to_asset,
			destination_address.clone(),
			0,
			1,
			Some(channel_metadata),
		));

		// CCM action is stored.
		let deposit_address = cf_test_utilities::assert_events_match!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::<Test>::StartWitnessing {
				deposit_address,
				opened_at,
				..
			}) if opened_at == BlockNumberProvider::get_block_height() => deposit_address
		);

		// Making a deposit should trigger CcmHandler.
		assert_ok!(IngressEgress::process_single_deposit(deposit_address, from_asset, amount, (),));
		assert_eq!(
			MockCcmHandler::get_ccm_requests(),
			vec![CcmRequest {
				source_asset: from_asset.into(),
				deposit_amount: amount,
				destination_asset: to_asset,
				destination_address,
				deposit_metadata: ccm,
				origin: SwapOrigin::DepositChannel {
					deposit_address: MockAddressConverter::to_encoded_address(
						deposit_address.into()
					),
					channel_id: 1,
				}
			}]
		);
	});
}

#[test]
fn can_egress_ccm() {
	new_test_ext().execute_with(|| {
		let destination_address: H160 = [0x01; 20].into();
		let destination_asset = eth::Asset::Eth;
		let ccm = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
			channel_metadata: CcmChannelMetadata {
				message: vec![0x00, 0x01, 0x02],
				gas_budget: 1_000,
				cf_parameters: vec![],
			}
		};
		let amount = 5_000;
		let egress_id = IngressEgress::schedule_egress(
			destination_asset,
			amount,
			destination_address,
			Some(ccm.clone())
		);

		assert!(ScheduledEgressFetchOrTransfer::<Test>::get().is_empty());
		assert_eq!(ScheduledEgressCcm::<Test>::get(), vec![
			CrossChainMessage {
				egress_id,
				asset: destination_asset,
				amount,
				destination_address,
				message: ccm.channel_metadata.message.clone(),
				cf_parameters: vec![],
				source_chain: ForeignChain::Ethereum,
				source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
			}
		]);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test>::EgressScheduled {
				id: egress_id,
				asset: destination_asset,
				amount,
				destination_address,
			}
		));

		// Send the scheduled ccm in on_finalize
		IngressEgress::on_finalize(1);

		// Check that the CCM should be egressed
		assert_eq!(MockEgressBroadcaster::get_pending_api_calls(), vec![<MockEthereumApiCall<MockEthEnvironment> as ExecutexSwapAndCall<Ethereum>>::new_unsigned(
			(ForeignChain::Ethereum, 1),
			TransferAssetParams {
				asset: destination_asset,
				amount,
				to: destination_address
			},
			ccm.source_chain,
			ccm.source_address,
			ccm.channel_metadata.message,
		).unwrap()]);

		// Storage should be cleared
		assert_eq!(ScheduledEgressCcm::<Test>::decode_len(), Some(0));
	});
}

#[test]
fn multi_use_deposit_address_different_blocks() {
	const ETH: eth::Asset = eth::Asset::Eth;

	new_test_ext()
		.then_execute_at_next_block(|_| request_address_and_deposit(ALICE, ETH))
		.then_execute_at_next_block(|channel @ (_, deposit_address)| {
			// Set the address to deployed.
			// Do another, should succeed.
			assert_ok!(Pallet::<Test, _>::process_single_deposit(deposit_address, ETH, 1, ()));
			channel
		})
		.then_execute_at_next_block(|(_, deposit_address)| {
			// Closing the channel should invalidate the deposit address.
			IngressEgress::close_channel(deposit_address);
			assert_noop!(
				IngressEgress::process_deposits(
					RuntimeOrigin::root(),
					vec![DepositWitness {
						deposit_address,
						asset: eth::Asset::Eth,
						amount: 1,
						deposit_details: Default::default()
					}],
					Default::default()
				),
				Error::<Test, _>::InvalidDepositAddress
			);
		});
}

#[test]
fn multi_use_deposit_same_block() {
	// Use FLIP because ETH doesn't trigger a second fetch.
	const FLIP: eth::Asset = eth::Asset::Flip;
	const DEPOSIT_AMOUNT: <Ethereum as Chain>::ChainAmount = 1_000;
	new_test_ext()
		.request_deposit_addresses(&[(ALICE, FLIP)])
		.map_context(|mut ctx| {
			assert!(ctx.len() == 1);
			ctx.pop().unwrap()
		})
		.inspect_storage(|(_, deposit_address, _)| {
			assert!(
				DepositChannelLookup::<Test, _>::get(deposit_address)
					.unwrap()
					.deposit_channel
					.state == cf_chains::eth::DeploymentStatus::Undeployed
			);
		})
		.then_apply_extrinsics(|&(_, deposit_address, asset)| {
			[(
				OriginTrait::root(),
				PalletCall::<Test, _>::process_deposits {
					deposit_witnesses: vec![
						DepositWitness {
							deposit_address,
							asset,
							amount: MinimumDeposit::<Test>::get(asset) + DEPOSIT_AMOUNT,
							deposit_details: Default::default(),
						},
						DepositWitness {
							deposit_address,
							asset,
							amount: MinimumDeposit::<Test>::get(asset) + DEPOSIT_AMOUNT,
							deposit_details: Default::default(),
						},
					],
					block_height: Default::default(),
				},
				Ok(()),
			)]
		})
		.inspect_storage(|(channel_id, deposit_address, _)| {
			assert_eq!(
				DepositChannelLookup::<Test, _>::get(deposit_address)
					.unwrap()
					.deposit_channel
					.state,
				cf_chains::eth::DeploymentStatus::Pending,
			);
			let scheduled_fetches = ScheduledEgressFetchOrTransfer::<Test, _>::get();
			let pending_api_calls = MockEgressBroadcaster::get_pending_api_calls();
			let pending_callbacks = MockEgressBroadcaster::get_pending_callbacks();
			assert!(scheduled_fetches.len() == 1);
			assert!(pending_api_calls.len() == 1);
			assert!(pending_callbacks.len() == 1);
			assert!(
				matches!(
					scheduled_fetches.last().unwrap(),
					FetchOrTransfer::Fetch {
						asset: FLIP,
						..
					}
				),
				"Expected one pending fetch to still be scheduled for the deposit address, got: {:?}",
				scheduled_fetches
			);
			assert!(
				matches!(
					pending_api_calls.last().unwrap(),
					MockEthereumApiCall::AllBatch(MockAllBatch {
						fetch_params,
						..
					}) if matches!(
						fetch_params.last().unwrap().deposit_fetch_id,
						EthereumFetchId::DeployAndFetch(id) if id == *channel_id
					)
				),
				"Expected one AllBatch apicall to be scheduled for address deployment, got {:?}.",
				pending_api_calls
			);
			assert!(matches!(
				pending_callbacks.last().unwrap(),
				RuntimeCall::IngressEgress(PalletCall::finalise_ingress { .. })
			));
		})
		.then_execute_at_next_block(|ctx| {
			MockEgressBroadcaster::dispatch_all_callbacks();
			ctx
		})
		.inspect_storage(|(_, deposit_address, _)| {
			assert_eq!(
				DepositChannelLookup::<Test, _>::get(deposit_address)
					.unwrap()
					.deposit_channel
					.state,
				cf_chains::eth::DeploymentStatus::Deployed
			);
			let scheduled_fetches = ScheduledEgressFetchOrTransfer::<Test, _>::get();
			let pending_api_calls = MockEgressBroadcaster::get_pending_api_calls();
			let pending_callbacks = MockEgressBroadcaster::get_pending_callbacks();
			assert!(scheduled_fetches.is_empty());
			assert!(pending_api_calls.len() == 2);
			assert!(pending_callbacks.len() == 1);
			assert!(
				matches!(
					&pending_api_calls[1],
					MockEthereumApiCall::AllBatch(MockAllBatch {
						fetch_params,
						..
					}) if matches!(
						fetch_params.last().unwrap().deposit_fetch_id,
						EthereumFetchId::Fetch(address) if address == *deposit_address
					)
				),
				"Expected a new AllBatch apicall to be scheduled to fetch from a deployed address, got {:?}.",
				pending_api_calls
			);
		});
}

#[test]
fn can_set_minimum_deposit() {
	new_test_ext().execute_with(|| {
		let asset = eth::Asset::Eth;
		let minimum_deposit = 1_500u128;
		assert_eq!(MinimumDeposit::<Test>::get(asset), 0);
		// Set the new minimum deposits
		assert_ok!(IngressEgress::set_minimum_deposit(
			RuntimeOrigin::root(),
			asset,
			minimum_deposit
		));

		assert_eq!(MinimumDeposit::<Test>::get(asset), minimum_deposit);

		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test>::MinimumDepositSet { asset, minimum_deposit },
		));
	});
}

#[test]
fn deposits_below_minimum_are_rejected() {
	new_test_ext().execute_with(|| {
		let eth = eth::Asset::Eth;
		let flip = eth::Asset::Flip;
		let default_deposit_amount = 1_000;

		// Set minimum deposit
		assert_ok!(IngressEgress::set_minimum_deposit(RuntimeOrigin::root(), eth, 1_500));
		assert_ok!(IngressEgress::set_minimum_deposit(
			RuntimeOrigin::root(),
			flip,
			default_deposit_amount
		));

		// Observe that eth deposit gets rejected.
		let (_, deposit_address) = request_address_and_deposit(0, eth);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test>::DepositIgnored {
				deposit_address,
				asset: eth,
				amount: default_deposit_amount,
				deposit_details: Default::default(),
			},
		));

		// Flip deposit should succeed.
		let (_, deposit_address) = request_address_and_deposit(0, flip);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test>::DepositReceived {
				deposit_address,
				asset: flip,
				amount: default_deposit_amount,
				deposit_details: Default::default(),
			},
		));
	});
}

#[test]
fn handle_pending_deployment() {
	const ETH: eth::Asset = eth::Asset::Eth;
	new_test_ext().execute_with(|| {
		// Initial request.
		let (_, deposit_address) = request_address_and_deposit(ALICE, eth::Asset::Eth);
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 1);
		// Process deposits.
		IngressEgress::on_finalize(1);
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 0);
		// Process deposit again the same address.
		Pallet::<Test, _>::process_single_deposit(deposit_address, ETH, 1, ()).unwrap();
		// None-pending requests can still be sent
		request_address_and_deposit(1u64, eth::Asset::Eth);
		request_address_and_deposit(2u64, eth::Asset::Eth);
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 3);
		// Process deposit again.
		IngressEgress::on_finalize(1);
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 1);
		// Now finalize the first fetch and deploy the address with that.
		assert_ok!(IngressEgress::finalise_ingress(RuntimeOrigin::root(), vec![deposit_address]));
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 1);
		// Process deposit again amd expect the fetch request to be picked up.
		IngressEgress::on_finalize(1);
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 0);
	});
}

#[test]
fn handle_pending_deployment_same_block() {
	new_test_ext().execute_with(|| {
		// Initial request.
		let (_, deposit_address) = request_address_and_deposit(ALICE, eth::Asset::Eth);
		Pallet::<Test, _>::process_single_deposit(deposit_address, eth::Asset::Eth, 1, ()).unwrap();
		// Expect to have two fetch requests.
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 2);
		// Process deposits.
		IngressEgress::on_finalize(1);
		// Expect only one fetch request processed in one block. Note: This not the most performant
		// solution, but also an edge case. Maybe we can improve this in the future.
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 1);
		// Process deposit (again).
		IngressEgress::on_finalize(2);
		// Expect the request still to be in the queue.
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 1);
		// Simulate the finalization of the first fetch request.
		assert_ok!(IngressEgress::finalise_ingress(RuntimeOrigin::root(), vec![deposit_address]));
		// Process deposit (again).
		IngressEgress::on_finalize(3);
		// All fetch requests should be processed.
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 0);
	});
}

#[test]
fn channel_reuse_with_different_assets() {
	const ASSET_1: eth::Asset = eth::Asset::Eth;
	const ASSET_2: eth::Asset = eth::Asset::Flip;
	new_test_ext()
		// First, request a deposit address and use it, then close it so it gets recycled.
		.request_address_and_deposit(&[(ALICE, ASSET_1, 100_000)])
		.map_context(|mut result| result.pop().unwrap())
		.then_execute_at_next_block(|ctx| {
			// Dispatch callbacks to finalise the ingress.
			MockEgressBroadcaster::dispatch_all_callbacks();
			ctx
		})
		.inspect_storage(|(_, address, asset)| {
			assert_eq!(*asset, ASSET_1);
			assert!(
				DepositChannelLookup::<Test, _>::get(address).unwrap().deposit_channel.asset ==
					*asset
			);
		})
		.then_execute_at_next_block(|(channel_id, channel_address, _)| {
			// Close the channel.
			IngressEgress::close_channel(channel_address);
			channel_id
		})
		.inspect_storage(|channel_id| {
			assert!(DepositChannelLookup::<Test, _>::get(ALICE_ETH_ADDRESS).is_none());
			assert!(
				DepositChannelPool::<Test, _>::iter_values().next().unwrap().channel_id ==
					*channel_id
			);
		})
		// Request a new address with a different asset.
		.request_deposit_addresses(&[(ALICE, ASSET_2)])
		.map_context(|mut result| result.pop().unwrap())
		// Ensure that the deposit channel's asset is updated.
		.inspect_storage(|(_, address, asset)| {
			assert_eq!(*asset, ASSET_2);
			assert!(
				DepositChannelLookup::<Test, _>::get(address).unwrap().deposit_channel.asset ==
					*asset
			);
		});
}
