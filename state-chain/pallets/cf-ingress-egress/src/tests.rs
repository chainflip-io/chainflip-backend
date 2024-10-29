mod boost;

use crate::{
	mock_eth::*, BoostPoolId, BoostStatus, Call as PalletCall, ChannelAction, ChannelIdCounter,
	ChannelOpeningFee, CrossChainMessage, DepositAction, DepositChannelLookup, DepositChannelPool,
	DepositIgnoredReason, DepositWitness, DisabledEgressAssets, EgressDustLimit,
	Event as PalletEvent, FailedForeignChainCall, FailedForeignChainCalls, FetchOrTransfer,
	MinimumDeposit, Pallet, PalletConfigUpdate, PalletSafeMode, PrewitnessedDepositIdCounter,
	ReportExpiresAt, ScheduledEgressCcm, ScheduledEgressFetchOrTransfer, ScheduledTxForReject,
	TaintedTransactionStatus, TaintedTransactions, TAINTED_TX_EXPIRATION_BLOCKS,
};
use cf_chains::{
	address::{AddressConverter, EncodedAddress},
	assets::eth::Asset as EthAsset,
	btc::{BitcoinNetwork, ScriptPubkey},
	evm::{self, DepositDetails, EvmFetchId},
	mocks::MockEthereum,
	CcmChannelMetadata, CcmFailReason, DepositChannel, ExecutexSwapAndCall, SwapOrigin,
	TransferAssetParams,
};
use cf_primitives::{AssetAmount, Beneficiaries, ChannelId, ForeignChain};
use cf_test_utilities::{assert_events_eq, assert_has_event, assert_has_matching_event};
use cf_traits::{
	mocks::{
		self,
		account_role_registry::MockAccountRoleRegistry,
		address_converter::MockAddressConverter,
		api_call::{MockEthAllBatch, MockEthereumApiCall, MockEvmEnvironment},
		asset_converter::MockAssetConverter,
		asset_withholding::MockAssetWithholding,
		balance_api::MockBalance,
		block_height_provider::BlockHeightProvider,
		chain_tracking::ChainTracker,
		fetches_transfers_limit_provider::MockFetchesTransfersLimitProvider,
		funding_info::MockFundingInfo,
		swap_request_api::{MockSwapRequest, MockSwapRequestHandler},
	},
	AccountRoleRegistry, BalanceApi, DepositApi, EgressApi, EpochInfo,
	FetchesTransfersLimitProvider, FundingInfo, GetBlockHeight, SafeMode, ScheduledEgressDetails,
	SwapRequestType,
};
use frame_support::{
	assert_err, assert_noop, assert_ok,
	traits::{Hooks, OriginTrait},
	weights::Weight,
};
use sp_core::{H160, H256};
use sp_runtime::{DispatchError, DispatchError::BadOrigin};

const ALICE_ETH_ADDRESS: EthereumAddress = H160([100u8; 20]);
const BOB_ETH_ADDRESS: EthereumAddress = H160([101u8; 20]);
const ETH_ETH: EthAsset = EthAsset::Eth;
const ETH_FLIP: EthAsset = EthAsset::Flip;
const DEFAULT_DEPOSIT_AMOUNT: u128 = 1_000;

#[track_caller]
fn expect_size_of_address_pool(size: usize) {
	assert_eq!(
		DepositChannelPool::<Test, ()>::iter_keys().count(),
		size,
		"Address pool size is incorrect!"
	);
}

#[test]
fn blacklisted_asset_will_not_egress_via_batch_all() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;

		// Cannot egress assets that are blacklisted.
		assert!(DisabledEgressAssets::<Test, ()>::get(asset).is_none());
		assert_ok!(IngressEgress::enable_or_disable_egress(RuntimeOrigin::root(), asset, true));
		assert!(DisabledEgressAssets::<Test, ()>::get(asset).is_some());
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::AssetEgressStatusChanged { asset, disabled: true },
		));

		// Eth should be blocked while Flip can be sent
		assert_ok!(IngressEgress::schedule_egress(asset, 1_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 1_000, ALICE_ETH_ADDRESS, None));

		IngressEgress::on_finalize(1);

		// The egress has not been sent
		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, ()>::get(),
			vec![FetchOrTransfer::<Ethereum>::Transfer {
				asset,
				amount: 1_000,
				destination_address: ALICE_ETH_ADDRESS,
				egress_id: (ForeignChain::Ethereum, 1),
			}]
		);

		// re-enable the asset for Egress
		assert_ok!(IngressEgress::enable_or_disable_egress(RuntimeOrigin::root(), asset, false));
		assert!(DisabledEgressAssets::<Test, ()>::get(asset).is_none());
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::AssetEgressStatusChanged { asset, disabled: false },
		));

		IngressEgress::on_finalize(1);

		// The egress should be sent now
		assert!(ScheduledEgressFetchOrTransfer::<Test, ()>::get().is_empty());
	});
}

#[test]
fn blacklisted_asset_will_not_egress_via_ccm() {
	new_test_ext().execute_with(|| {
		let asset = ETH_ETH;
		let gas_budget = 1000u128;
		let ccm = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
			channel_metadata: CcmChannelMetadata {
				message: vec![0x00, 0x01, 0x02].try_into().unwrap(),
				gas_budget: 1_000,
				ccm_additional_data: vec![].try_into().unwrap(),
			},
		};

		assert!(DisabledEgressAssets::<Test, ()>::get(asset).is_none());
		assert_ok!(IngressEgress::enable_or_disable_egress(RuntimeOrigin::root(), asset, true));

		// Eth should be blocked while Flip can be sent
		assert_ok!(IngressEgress::schedule_egress(
			asset,
			1_000,
			ALICE_ETH_ADDRESS,
			Some((ccm.clone(), gas_budget)),
		));
		assert_ok!(IngressEgress::schedule_egress(
			ETH_FLIP,
			1_000,
			ALICE_ETH_ADDRESS,
			Some((ccm.clone(), gas_budget)),
		));

		IngressEgress::on_finalize(1);

		// The egress has not been sent
		assert_eq!(
			ScheduledEgressCcm::<Test, ()>::get(),
			vec![CrossChainMessage {
				egress_id: (ForeignChain::Ethereum, 1),
				asset,
				amount: 1_000,
				destination_address: ALICE_ETH_ADDRESS,
				message: ccm.channel_metadata.message.clone(),
				source_chain: ForeignChain::Ethereum,
				source_address: ccm.source_address.clone(),
				ccm_additional_data: ccm.channel_metadata.ccm_additional_data,
				gas_budget,
			}]
		);

		// re-enable the asset for Egress
		assert_ok!(IngressEgress::enable_or_disable_egress(RuntimeOrigin::root(), asset, false));

		IngressEgress::on_finalize(2);

		// The egress should be sent now
		assert!(ScheduledEgressCcm::<Test, ()>::get().is_empty());
	});
}

#[test]
fn egress_below_minimum_deposit_ignored() {
	new_test_ext().execute_with(|| {
		const MIN_EGRESS: u128 = 1_000;
		const AMOUNT: u128 = MIN_EGRESS - 1;

		EgressDustLimit::<Test, ()>::set(ETH_ETH, MIN_EGRESS);

		assert_err!(
			IngressEgress::schedule_egress(ETH_ETH, AMOUNT, ALICE_ETH_ADDRESS, None),
			crate::Error::<Test, _>::BelowEgressDustLimit
		);

		assert!(ScheduledEgressFetchOrTransfer::<Test, ()>::get().is_empty());
	});
}

#[test]
fn can_schedule_swap_egress_to_batch() {
	new_test_ext().execute_with(|| {
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 3_000, BOB_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 4_000, BOB_ETH_ADDRESS, None));

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, ()>::get(),
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
	asset: EthAsset,
) -> (ChannelId, <Ethereum as Chain>::ChainAccount) {
	let (id, address, ..) = IngressEgress::request_liquidity_deposit_address(
		who,
		asset,
		0,
		ForeignChainAddress::Eth(Default::default()),
	)
	.unwrap();
	let address: <Ethereum as Chain>::ChainAccount = address.try_into().unwrap();
	assert_ok!(IngressEgress::process_single_deposit(
		address,
		asset,
		DEFAULT_DEPOSIT_AMOUNT,
		Default::default(),
		Default::default()
	));
	(id, address)
}

#[test]
fn can_schedule_deposit_fetch() {
	new_test_ext().execute_with(|| {
		assert!(ScheduledEgressFetchOrTransfer::<Test, ()>::get().is_empty());

		request_address_and_deposit(1u64, EthAsset::Eth);
		request_address_and_deposit(2u64, EthAsset::Eth);
		request_address_and_deposit(3u64, EthAsset::Flip);

		assert!(matches!(
			&ScheduledEgressFetchOrTransfer::<Test, ()>::get()[..],
			&[
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_ETH, .. },
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_ETH, .. },
				FetchOrTransfer::<Ethereum>::Fetch { asset: ETH_FLIP, .. },
			]
		));

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(
			crate::Event::DepositFetchesScheduled { channel_id: 1, asset: EthAsset::Eth },
		));

		request_address_and_deposit(4u64, EthAsset::Eth);

		assert!(matches!(
			&ScheduledEgressFetchOrTransfer::<Test, ()>::get()[..],
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
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS, None));
		request_address_and_deposit(1u64, EthAsset::Eth);
		request_address_and_deposit(2u64, EthAsset::Eth);
		request_address_and_deposit(3u64, EthAsset::Eth);
		request_address_and_deposit(4u64, EthAsset::Eth);

		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS, None));
		request_address_and_deposit(5u64, EthAsset::Flip);

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

		assert!(ScheduledEgressFetchOrTransfer::<Test, ()>::get().is_empty());
	});
}

#[test]
fn all_batch_apicall_creation_failure_should_rollback_storage() {
	new_test_ext().execute_with(|| {
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 2_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 3_000, BOB_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 4_000, BOB_ETH_ADDRESS, None));
		request_address_and_deposit(1u64, EthAsset::Eth);
		request_address_and_deposit(2u64, EthAsset::Eth);
		request_address_and_deposit(3u64, EthAsset::Eth);
		request_address_and_deposit(4u64, EthAsset::Eth);

		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 5_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 6_000, ALICE_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 7_000, BOB_ETH_ADDRESS, None));
		assert_ok!(IngressEgress::schedule_egress(ETH_FLIP, 8_000, BOB_ETH_ADDRESS, None));
		request_address_and_deposit(5u64, EthAsset::Flip);

		MockEthAllBatch::<MockEvmEnvironment>::set_success(false);
		request_address_and_deposit(4u64, EthAsset::Usdc);

		let scheduled_requests = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		// Try to send the scheduled egresses via Allbatch apicall. Will fail and so should rollback
		// the ScheduledEgressFetchOrTransfer
		IngressEgress::on_finalize(1);

		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, ()>::get(), scheduled_requests);
	});
}

#[test]
fn addresses_are_getting_reused() {
	new_test_ext()
		// Request 2 deposit addresses and deposit to one of them.
		.request_address_and_deposit(&[
			(DepositRequest::Liquidity { lp_account: ALICE, asset: EthAsset::Eth }, 100u32.into()),
			(DepositRequest::Liquidity { lp_account: ALICE, asset: EthAsset::Eth }, 0u32.into()),
		])
		.then_execute_with_keep_context(|deposit_details| {
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
			// This would normally be triggered on broadcast success, should finalise the ingress.
			for id in broadcast_ids {
				MockEgressBroadcaster::dispatch_success_callback(id);
			}
			channels
		})
		.then_execute_at_next_block(|channels| {
			let recycle_block = IngressEgress::expiry_and_recycle_block_height().2;
			BlockHeightProvider::<MockEthereum>::set_block_height(recycle_block);

			channels[0].clone()
		})
		// Check that the used address is now deployed and in the pool of available addresses.
		.then_execute_with_keep_context(|(_request, channel_id, address)| {
			expect_size_of_address_pool(1);
			// Address 1 is free to use and in the pool of available addresses
			assert_eq!(DepositChannelPool::<Test, _>::get(channel_id).unwrap().address, *address);
		})
		.request_deposit_addresses(&[DepositRequest::SimpleSwap {
			source_asset: EthAsset::Eth,
			destination_asset: EthAsset::Flip,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}])
		// The address should have been taken from the pool and the id counter unchanged.
		.then_execute_with_keep_context(|_| {
			expect_size_of_address_pool(0);
			assert_eq!(ChannelIdCounter::<Test, _>::get(), 2);
		});
}

#[test]
fn proof_address_pool_integrity() {
	new_test_ext().execute_with(|| {
		let channel_details = (0..3)
			.map(|id| request_address_and_deposit(id, EthAsset::Eth))
			.collect::<Vec<_>>();
		// All addresses in use
		expect_size_of_address_pool(0);
		IngressEgress::on_finalize(1);
		for (_id, address) in channel_details {
			assert_ok!(IngressEgress::finalise_ingress(RuntimeOrigin::root(), vec![address]));
		}
		let recycle_block = IngressEgress::expiry_and_recycle_block_height().2;
		BlockHeightProvider::<MockEthereum>::set_block_height(recycle_block);

		IngressEgress::on_idle(1, Weight::MAX);

		// Expect all addresses to be available
		expect_size_of_address_pool(3);
		request_address_and_deposit(4u64, EthAsset::Eth);
		// Expect one address to be in use
		expect_size_of_address_pool(2);
	});
}

#[test]
fn create_new_address_while_pool_is_empty() {
	new_test_ext().execute_with(|| {
		let channel_details = (0..2)
			.map(|id| request_address_and_deposit(id, EthAsset::Eth))
			.collect::<Vec<_>>();
		IngressEgress::on_finalize(1);
		for (_id, address) in channel_details {
			assert_ok!(IngressEgress::finalise_ingress(RuntimeOrigin::root(), vec![address]));
		}
		let recycle_block = IngressEgress::expiry_and_recycle_block_height().2;
		BlockHeightProvider::<MockEthereum>::set_block_height(recycle_block);
		IngressEgress::on_idle(1, Weight::MAX);

		assert_eq!(ChannelIdCounter::<Test, ()>::get(), 2);
		request_address_and_deposit(3u64, EthAsset::Eth);
		assert_eq!(ChannelIdCounter::<Test, ()>::get(), 2);
		IngressEgress::on_finalize(1);
		assert_eq!(ChannelIdCounter::<Test, ()>::get(), 2);
	});
}

#[test]
fn reused_address_channel_id_matches() {
	new_test_ext().execute_with(|| {
		const CHANNEL_ID: ChannelId = 0;
		let new_channel = DepositChannel::<Ethereum>::generate_new::<
			<Test as crate::Config>::AddressDerivation,
		>(CHANNEL_ID, EthAsset::Eth)
		.unwrap();
		DepositChannelPool::<Test, _>::insert(CHANNEL_ID, new_channel.clone());
		let (reused_channel_id, reused_address, ..) = IngressEgress::open_channel(
			&ALICE,
			EthAsset::Eth,
			ChannelAction::LiquidityProvision {
				lp_account: 0,
				refund_address: Some(ForeignChainAddress::Eth([0u8; 20].into())),
			},
			0,
		)
		.unwrap();
		// The reused details should be the same as before.
		assert_eq!(new_channel.channel_id, reused_channel_id);
		assert_eq!(new_channel.address, reused_address);
	});
}

#[test]
fn can_egress_ccm() {
	new_test_ext().execute_with(|| {
		let destination_address: H160 = [0x01; 20].into();
		let destination_asset = EthAsset::Eth;
		const GAS_BUDGET: u128 = 1_000;
		let ccm = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
			channel_metadata: CcmChannelMetadata {
				message: vec![0x00, 0x01, 0x02].try_into().unwrap(),
				gas_budget: GAS_BUDGET,
				ccm_additional_data: vec![].try_into().unwrap(),
			}
		};

		let amount = 5_000;
		let ScheduledEgressDetails { egress_id, .. } = IngressEgress::schedule_egress(
			destination_asset,
			amount,
			destination_address,
			Some((ccm.clone(), GAS_BUDGET))
		).expect("Egress should succeed");

		assert!(ScheduledEgressFetchOrTransfer::<Test, ()>::get().is_empty());
		assert_eq!(ScheduledEgressCcm::<Test, ()>::get(), vec![
			CrossChainMessage {
				egress_id,
				asset: destination_asset,
				amount,
				destination_address,
				message: ccm.channel_metadata.message.clone(),
				ccm_additional_data: vec![].try_into().unwrap(),
				source_chain: ForeignChain::Ethereum,
				source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
				gas_budget: GAS_BUDGET,
			}
		]);

		// Send the scheduled ccm in on_finalize
		IngressEgress::on_finalize(1);

		// Check that the CCM should be egressed
		assert_eq!(MockEgressBroadcaster::get_pending_api_calls(), vec![<MockEthereumApiCall<MockEvmEnvironment> as ExecutexSwapAndCall<Ethereum>>::new_unsigned(
			TransferAssetParams {
				asset: destination_asset,
				amount,
				to: destination_address
			},
			ccm.source_chain,
			ccm.source_address,
			GAS_BUDGET,
			ccm.channel_metadata.message.to_vec(),
			vec![],
		).unwrap()]);

		// Storage should be cleared
		assert_eq!(ScheduledEgressCcm::<Test, ()>::decode_len(), Some(0));
	});
}

#[test]
fn multi_deposit_includes_deposit_beyond_recycle_height() {
	const ETH: EthAsset = EthAsset::Eth;
	new_test_ext()
		.then_execute_at_next_block(|_| {
			let (_, address, ..) = IngressEgress::request_liquidity_deposit_address(
				ALICE,
				ETH,
				0,
				ForeignChainAddress::Eth(Default::default()),
			)
			.unwrap();
			let address: <Ethereum as Chain>::ChainAccount = address.try_into().unwrap();
			let recycles_at = IngressEgress::expiry_and_recycle_block_height().2;
			(address, recycles_at)
		})
		.then_execute_at_next_block(|(address, recycles_at)| {
			BlockHeightProvider::<MockEthereum>::set_block_height(recycles_at);
			address
		})
		.then_execute_at_next_block(|address| {
			let (_, address2, ..) = IngressEgress::request_liquidity_deposit_address(
				ALICE,
				ETH,
				0,
				ForeignChainAddress::Eth(Default::default()),
			)
			.unwrap();
			let address2: <Ethereum as Chain>::ChainAccount = address2.try_into().unwrap();
			(address, address2)
		})
		.then_execute_at_next_block(|(address, address2)| {
			IngressEgress::process_deposit_witnesses(
				vec![
					DepositWitness {
						deposit_address: address,
						asset: ETH,
						amount: 1,
						deposit_details: Default::default(),
					},
					DepositWitness {
						deposit_address: address2,
						asset: ETH,
						amount: 1,
						deposit_details: Default::default(),
					},
				],
				// block height is purely informative.
				BlockHeightProvider::<MockEthereum>::get_block_height(),
			)
			.unwrap();
			(address, address2)
		})
		.then_process_events(|_, event| match event {
			RuntimeEvent::IngressEgress(crate::Event::DepositWitnessRejected { .. }) |
			RuntimeEvent::IngressEgress(crate::Event::DepositFinalised { .. }) => Some(event),
			_ => None,
		})
		.inspect_context(|((expected_rejected_address, expected_accepted_address), emitted)| {
			assert_eq!(emitted.len(), 2);
			assert!(emitted.iter().any(|e| matches!(
			e,
			RuntimeEvent::IngressEgress(
				crate::Event::DepositWitnessRejected {
					deposit_witness,
					..
				}) if deposit_witness.deposit_address == *expected_rejected_address
			)),);
			assert!(emitted.iter().any(|e| matches!(
			e,
			RuntimeEvent::IngressEgress(
				crate::Event::DepositFinalised {
					deposit_address,
					..
				}) if deposit_address == expected_accepted_address
			)),);
		});
}

#[test]
fn multi_use_deposit_address_different_blocks() {
	const ETH: EthAsset = EthAsset::Eth;

	new_test_ext()
		.then_execute_at_next_block(|_| request_address_and_deposit(ALICE, ETH))
		.then_execute_at_next_block(|(_, deposit_address)| {
			IngressEgress::process_deposit_witnesses(
				vec![DepositWitness {
					deposit_address,
					asset: ETH,
					amount: 1,
					deposit_details: Default::default(),
				}],
				// block height is purely informative.
				BlockHeightProvider::<MockEthereum>::get_block_height(),
			)
			.unwrap();
			deposit_address
		})
		.then_execute_at_next_block(|deposit_address| {
			assert_ok!(Pallet::<Test, _>::process_single_deposit(
				deposit_address,
				ETH,
				1,
				Default::default(),
				Default::default()
			));
			assert!(
				MockBalance::get_balance(&ALICE, ETH.into()) > 0,
				"LP account hasn't earned fees!"
			);
			let recycle_block = IngressEgress::expiry_and_recycle_block_height().2;
			BlockHeightProvider::<MockEthereum>::set_block_height(recycle_block);

			deposit_address
		})
		// The channel should be closed at the next block.
		.then_execute_at_next_block(|deposit_address| {
			IngressEgress::process_deposit_witnesses(
				vec![DepositWitness {
					deposit_address,
					asset: ETH,
					amount: 1,
					deposit_details: Default::default(),
				}],
				// block height is purely informative.
				BlockHeightProvider::<MockEthereum>::get_block_height(),
			)
			.unwrap();
			deposit_address
		})
		.then_process_events(|_, event| match event {
			RuntimeEvent::IngressEgress(crate::Event::DepositWitnessRejected {
				deposit_witness,
				..
			}) => Some(deposit_witness.deposit_address),
			_ => None,
		})
		.inspect_context(|(expected_address, emitted)| {
			assert_eq!(*emitted, vec![*expected_address]);
		});
}

#[test]
fn multi_use_deposit_same_block() {
	// Use FLIP because ETH doesn't trigger a second fetch.
	const FLIP: EthAsset = EthAsset::Flip;
	const DEPOSIT_AMOUNT: <Ethereum as Chain>::ChainAmount = 1_000;
	new_test_ext()
		.request_deposit_addresses(&[DepositRequest::Liquidity { lp_account: ALICE, asset: FLIP }])
		.map_context(|mut ctx| {
			assert!(ctx.len() == 1);
			ctx.pop().unwrap()
		})
		.then_execute_with_keep_context(|(_, _, deposit_address)| {
			assert!(
				DepositChannelLookup::<Test, _>::get(deposit_address)
					.unwrap()
					.deposit_channel
					.state == cf_chains::evm::DeploymentStatus::Undeployed
			);
		})
		.then_execute_at_next_block(|(request, channel_id, deposit_address)| {
			let asset = request.source_asset();
			IngressEgress::process_deposit_witnesses(
				vec![
					DepositWitness {
						deposit_address,
						asset,
						amount: MinimumDeposit::<Test, ()>::get(asset) + DEPOSIT_AMOUNT,
						deposit_details: Default::default(),
					},
					DepositWitness {
						deposit_address,
						asset,
						amount: MinimumDeposit::<Test, ()>::get(asset) + DEPOSIT_AMOUNT,
						deposit_details: Default::default(),
					},
				],
				Default::default(),
			)
			.unwrap();
			(request, channel_id, deposit_address)
		})
		.then_execute_with_keep_context(|(_, channel_id, deposit_address)| {
			assert_eq!(
				DepositChannelLookup::<Test, _>::get(deposit_address)
					.unwrap()
					.deposit_channel
					.state,
				cf_chains::evm::DeploymentStatus::Pending,
			);
			let scheduled_fetches = ScheduledEgressFetchOrTransfer::<Test, _>::get();
			let pending_api_calls = MockEgressBroadcaster::get_pending_api_calls();
			let pending_callbacks = MockEgressBroadcaster::get_success_pending_callbacks();
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
					MockEthereumApiCall::AllBatch(MockEthAllBatch {
						fetch_params,
						..
					}) if matches!(
						fetch_params.last().unwrap().deposit_fetch_id,
						EvmFetchId::DeployAndFetch(id) if id == *channel_id
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
			MockEgressBroadcaster::dispatch_all_success_callbacks();
			ctx
		})
		.then_execute_with_keep_context(|(_, _, deposit_address)| {
			assert_eq!(
				DepositChannelLookup::<Test, _>::get(deposit_address)
					.unwrap()
					.deposit_channel
					.state,
				cf_chains::evm::DeploymentStatus::Deployed
			);
			let scheduled_fetches = ScheduledEgressFetchOrTransfer::<Test, _>::get();
			let pending_api_calls = MockEgressBroadcaster::get_pending_api_calls();
			let pending_callbacks = MockEgressBroadcaster::get_success_pending_callbacks();
			assert!(scheduled_fetches.is_empty());
			assert!(pending_api_calls.len() == 2);
			assert!(pending_callbacks.len() == 1);
			assert!(
				matches!(
					&pending_api_calls[1],
					MockEthereumApiCall::AllBatch(MockEthAllBatch {
						fetch_params,
						..
					}) if matches!(
						fetch_params.last().unwrap().deposit_fetch_id,
						EvmFetchId::Fetch(address) if address == *deposit_address
					)
				),
				"Expected a new AllBatch apicall to be scheduled to fetch from a deployed address, got {:?}.",
				pending_api_calls
			);
		});
}

#[test]
fn deposits_below_minimum_are_rejected() {
	new_test_ext().execute_with(|| {
		let eth = EthAsset::Eth;
		let flip = EthAsset::Flip;
		let default_deposit_amount = 1_000;

		// Set minimum deposit
		assert_ok!(IngressEgress::update_pallet_config(
			RuntimeOrigin::root(),
			vec![
				PalletConfigUpdate::<Test, _>::SetMinimumDeposit {
					asset: eth,
					minimum_deposit: 1_500
				},
				PalletConfigUpdate::<Test, _>::SetMinimumDeposit {
					asset: flip,
					minimum_deposit: default_deposit_amount
				},
			]
			.try_into()
			.unwrap()
		));

		// Observe that eth deposit gets rejected.
		let (_, deposit_address) = request_address_and_deposit(0, eth);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, ()>::DepositIgnored {
				deposit_address: Some(deposit_address),
				asset: eth,
				amount: default_deposit_amount,
				deposit_details: Default::default(),
				reason: DepositIgnoredReason::BelowMinimumDeposit,
			},
		));

		const LP_ACCOUNT: u64 = 0;
		// Flip deposit should succeed.
		let (channel_id, deposit_address) = request_address_and_deposit(LP_ACCOUNT, flip);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, ()>::DepositFinalised {
				deposit_address,
				asset: flip,
				amount: default_deposit_amount,
				block_height: Default::default(),
				deposit_details: Default::default(),
				ingress_fee: 0,
				action: DepositAction::LiquidityProvision { lp_account: LP_ACCOUNT },
				channel_id,
			},
		));
	});
}

#[test]
fn deposits_ingress_fee_exceeding_deposit_amount_rejected() {
	const ASSET: EthAsset = EthAsset::Eth;
	const DEPOSIT_AMOUNT: u128 = 500;
	const HIGH_FEE: u128 = DEPOSIT_AMOUNT * 2;
	const LOW_FEE: u128 = DEPOSIT_AMOUNT / 10;

	new_test_ext().execute_with(|| {
		// Set fee to be higher than the deposit value.
		ChainTracker::<Ethereum>::set_fee(HIGH_FEE);

		let (_id, address, ..) = IngressEgress::request_liquidity_deposit_address(
			ALICE,
			ASSET,
			0,
			ForeignChainAddress::Eth(Default::default()),
		)
		.unwrap();
		let deposit_address = address.try_into().unwrap();

		// Swap a low enough amount such that it gets swallowed by fees
		let deposit_detail: DepositWitness<Ethereum> = DepositWitness::<Ethereum> {
			deposit_address,
			asset: ASSET,
			amount: DEPOSIT_AMOUNT,
			deposit_details: Default::default(),
		};
		assert_ok!(IngressEgress::process_deposit_witnesses(
			vec![deposit_detail.clone()],
			Default::default(),
		));
		// Observe the DepositIgnored Event
		assert!(
			matches!(
				cf_test_utilities::last_event::<Test>(),
				RuntimeEvent::IngressEgress(crate::Event::<Test, ()>::DepositIgnored {
					asset: ASSET,
					amount: DEPOSIT_AMOUNT,
					deposit_details: DepositDetails { tx_hashes: None },
					reason: DepositIgnoredReason::NotEnoughToPayFees,
					..
				},)
			),
			"Expected DepositIgnored Event, got: {:?}",
			cf_test_utilities::last_event::<Test>()
		);

		// Set fees to less than the deposit amount and retry.
		ChainTracker::<Ethereum>::set_fee(LOW_FEE);

		assert_ok!(IngressEgress::process_deposit_witnesses(
			vec![deposit_detail],
			Default::default(),
		));
		// Observe the DepositReceived Event
		assert!(
			matches!(
				cf_test_utilities::last_event::<Test>(),
				RuntimeEvent::IngressEgress(crate::Event::<Test, ()>::DepositFinalised {
					asset: ASSET,
					amount: DEPOSIT_AMOUNT,
					deposit_details: DepositDetails { tx_hashes: None },
					ingress_fee: LOW_FEE,
					action: DepositAction::LiquidityProvision { lp_account: ALICE },
					..
				},)
			),
			"Expected DepositReceived Event, got: {:?}",
			cf_test_utilities::last_event::<Test>()
		);
	});
}

#[test]
fn handle_pending_deployment() {
	const ETH: EthAsset = EthAsset::Eth;
	new_test_ext().execute_with(|| {
		// Initial request.
		let (_, deposit_address) = request_address_and_deposit(ALICE, EthAsset::Eth);
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 1);
		// Process deposits.
		IngressEgress::on_finalize(1);
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, _>::decode_len().unwrap_or_default(), 0);
		// Process deposit again the same address.
		Pallet::<Test, _>::process_single_deposit(
			deposit_address,
			ETH,
			1,
			Default::default(),
			Default::default(),
		)
		.unwrap();
		assert!(MockBalance::get_balance(&ALICE, ETH.into()) > 0, "LP account hasn't earned fees!");
		// None-pending requests can still be sent
		request_address_and_deposit(1u64, EthAsset::Eth);
		request_address_and_deposit(2u64, EthAsset::Eth);
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
		let (_, deposit_address) = request_address_and_deposit(ALICE, EthAsset::Eth);
		Pallet::<Test, _>::process_single_deposit(
			deposit_address,
			EthAsset::Eth,
			1,
			Default::default(),
			Default::default(),
		)
		.unwrap();
		assert!(
			MockBalance::get_balance(&ALICE, EthAsset::Eth.into()) > 0,
			"LP account hasn't earned fees!"
		);
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
	const ASSET_1: EthAsset = EthAsset::Eth;
	const ASSET_2: EthAsset = EthAsset::Flip;
	new_test_ext()
		// First, request a deposit address and use it, then close it so it gets recycled.
		.request_address_and_deposit(&[(
			DepositRequest::Liquidity { lp_account: ALICE, asset: ASSET_1 },
			100_000,
		)])
		.map_context(|mut result| result.pop().unwrap())
		.then_execute_at_next_block(|ctx| {
			// Dispatch callbacks to finalise the ingress.
			MockEgressBroadcaster::dispatch_all_success_callbacks();
			ctx
		})
		.then_execute_with_keep_context(|(request, _, address)| {
			let asset = request.source_asset();
			assert_eq!(asset, ASSET_1);
			assert!(
				DepositChannelLookup::<Test, _>::get(address).unwrap().deposit_channel.asset ==
					asset
			);
		})
		.then_execute_at_next_block(|(_, channel_id, _)| {
			let recycle_block = IngressEgress::expiry_and_recycle_block_height().2;
			BlockHeightProvider::<MockEthereum>::set_block_height(recycle_block);
			channel_id
		})
		.then_execute_with_keep_context(|channel_id| {
			assert!(DepositChannelLookup::<Test, _>::get(ALICE_ETH_ADDRESS).is_none());
			assert!(
				DepositChannelPool::<Test, _>::iter_values().next().unwrap().channel_id ==
					*channel_id
			);
		})
		// Request a new address with a different asset.
		.request_deposit_addresses(&[DepositRequest::Liquidity {
			lp_account: ALICE,
			asset: ASSET_2,
		}])
		.map_context(|mut result| result.pop().unwrap())
		// Ensure that the deposit channel's asset is updated.
		.then_execute_with_keep_context(|(request, _, address)| {
			let asset = request.source_asset();
			assert_eq!(asset, ASSET_2);
			assert!(
				DepositChannelLookup::<Test, _>::get(address).unwrap().deposit_channel.asset ==
					asset
			);
		});
}

/// This is the sequence we're testing.
/// 1. Request deposit address
/// 2. Deposit to address when it's almost expired
/// 3. The channel is expired
/// 4. We need to finalise the ingress, by fetching
/// 5. The fetch should succeed.
#[test]
fn ingress_finalisation_succeeds_after_channel_expired_but_not_recycled() {
	new_test_ext().execute_with(|| {
		assert!(
			ScheduledEgressFetchOrTransfer::<Test, ()>::get().is_empty(),
			"Is empty after genesis"
		);

		request_address_and_deposit(ALICE, EthAsset::Eth);

		// Because we're only *expiring* and not recycling, we should still be able to fetch.
		let expiry_block = IngressEgress::expiry_and_recycle_block_height().1;
		BlockHeightProvider::<MockEthereum>::set_block_height(expiry_block);

		IngressEgress::on_idle(1, Weight::MAX);

		IngressEgress::on_finalize(1);

		assert!(ScheduledEgressFetchOrTransfer::<Test, ()>::get().is_empty(),);
	});
}

#[test]
fn can_store_failed_vault_transfers() {
	new_test_ext().execute_with(|| {
		let epoch = MockEpochInfo::epoch_index();
		let asset = EthAsset::Eth;
		let amount = 1_000_000u128;
		let destination_address = [0xcf; 20].into();

		assert_ok!(IngressEgress::vault_transfer_failed(
			RuntimeOrigin::root(),
			asset,
			amount,
			destination_address,
		));

		let broadcast_id = 1;
		assert_has_event::<Test>(RuntimeEvent::IngressEgress(
			PalletEvent::TransferFallbackRequested {
				asset,
				amount,
				destination_address,
				broadcast_id,
			},
		));
		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch),
			vec![FailedForeignChainCall { broadcast_id, original_epoch: epoch }]
		);
	});
}

#[test]
fn test_default_empty_amounts() {
	let mut channel_recycle_blocks = Default::default();
	let can_recycle = IngressEgress::take_recyclable_addresses(&mut channel_recycle_blocks, 0, 0);

	assert_eq!(can_recycle, vec![]);
	assert_eq!(channel_recycle_blocks, vec![]);
}

#[test]
fn test_cannot_recycle_if_block_number_less_than_current_height() {
	let maximum_recyclable_number = 2;
	let mut channel_recycle_blocks =
		(1u64..5).map(|i| (i, H160::from([i as u8; 20]))).collect::<Vec<_>>();
	let current_block_height = 3;

	let can_recycle = IngressEgress::take_recyclable_addresses(
		&mut channel_recycle_blocks,
		maximum_recyclable_number,
		current_block_height,
	);

	assert_eq!(can_recycle, vec![H160::from([1u8; 20]), H160::from([2; 20])]);
	assert_eq!(
		channel_recycle_blocks,
		vec![(3, H160::from([3u8; 20])), (4, H160::from([4u8; 20]))]
	);
}

// Same test as above, but lower maximum recyclable number
#[test]
fn test_can_only_recycle_up_to_max_amount() {
	let maximum_recyclable_number = 1;
	let mut channel_recycle_blocks =
		(1u64..5).map(|i| (i, H160::from([i as u8; 20]))).collect::<Vec<_>>();
	let current_block_height = 3;

	let can_recycle = IngressEgress::take_recyclable_addresses(
		&mut channel_recycle_blocks,
		maximum_recyclable_number,
		current_block_height,
	);

	assert_eq!(can_recycle, vec![H160::from([1u8; 20])]);
	assert_eq!(
		channel_recycle_blocks,
		vec![(2, H160::from([2; 20])), (3, H160::from([3u8; 20])), (4, H160::from([4u8; 20]))]
	);
}

#[test]
fn none_can_be_recycled_due_to_low_block_number() {
	let maximum_recyclable_number = 4;
	let mut channel_recycle_blocks =
		(1u64..5).map(|i| (i, H160::from([i as u8; 20]))).collect::<Vec<_>>();
	let current_block_height = 0;

	let can_recycle = IngressEgress::take_recyclable_addresses(
		&mut channel_recycle_blocks,
		maximum_recyclable_number,
		current_block_height,
	);

	assert!(can_recycle.is_empty());
	assert_eq!(
		channel_recycle_blocks,
		vec![
			(1, H160::from([1u8; 20])),
			(2, H160::from([2; 20])),
			(3, H160::from([3; 20])),
			(4, H160::from([4; 20]))
		]
	);
}

#[test]
fn all_can_be_recycled() {
	let maximum_recyclable_number = 4;
	let mut channel_recycle_blocks =
		(1u64..5).map(|i| (i, H160::from([i as u8; 20]))).collect::<Vec<_>>();
	let current_block_height = 4;

	let can_recycle = IngressEgress::take_recyclable_addresses(
		&mut channel_recycle_blocks,
		maximum_recyclable_number,
		current_block_height,
	);

	assert_eq!(
		can_recycle,
		vec![H160::from([1u8; 20]), H160::from([2; 20]), H160::from([3; 20]), H160::from([4; 20])]
	);
	assert!(channel_recycle_blocks.is_empty());
}

#[test]
fn failed_ccm_is_stored() {
	new_test_ext().execute_with(|| {
		let epoch = MockEpochInfo::epoch_index();
		let broadcast_id = 1;
		assert_eq!(FailedForeignChainCalls::<Test, ()>::get(epoch), vec![]);

		assert_noop!(
			IngressEgress::ccm_broadcast_failed(RuntimeOrigin::signed(ALICE), broadcast_id,),
			sp_runtime::DispatchError::BadOrigin
		);
		assert_ok!(IngressEgress::ccm_broadcast_failed(RuntimeOrigin::root(), broadcast_id,));

		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch),
			vec![FailedForeignChainCall { broadcast_id, original_epoch: epoch }]
		);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, ()>::CcmBroadcastFailed { broadcast_id },
		));
	});
}

#[test]
fn on_finalize_handles_failed_calls() {
	new_test_ext().execute_with(|| {
		// Advance to Epoch 1 so the expiry logic start to work.
		let epoch = 1u32;
		MockEpochInfo::set_epoch(epoch);
		let destination_address = [0xcf; 20].into();
		assert_eq!(FailedForeignChainCalls::<Test, ()>::get(epoch), vec![]);

		assert_ok!(IngressEgress::vault_transfer_failed(
			RuntimeOrigin::root(),
			EthAsset::Eth,
			1_000_000,
			destination_address
		));
		assert_ok!(IngressEgress::ccm_broadcast_failed(RuntimeOrigin::root(), 12,));
		assert_ok!(IngressEgress::ccm_broadcast_failed(RuntimeOrigin::root(), 13,));

		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch),
			vec![
				FailedForeignChainCall { broadcast_id: 1, original_epoch: epoch },
				FailedForeignChainCall { broadcast_id: 12, original_epoch: epoch },
				FailedForeignChainCall { broadcast_id: 13, original_epoch: epoch }
			]
		);

		// on-finalize do nothing
		IngressEgress::on_finalize(0);

		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch),
			vec![
				FailedForeignChainCall { broadcast_id: 1, original_epoch: epoch },
				FailedForeignChainCall { broadcast_id: 12, original_epoch: epoch },
				FailedForeignChainCall { broadcast_id: 13, original_epoch: epoch }
			]
		);

		// Advance into the next epoch
		MockEpochInfo::set_epoch(epoch + 1);

		// Resign 1 call per block
		IngressEgress::on_finalize(1);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, ()>::FailedForeignChainCallResigned {
				broadcast_id: 13,
				threshold_signature_id: 2,
			},
		));
		assert_eq!(MockEgressBroadcaster::resigned_call(), Some(13u32));
		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch),
			vec![
				FailedForeignChainCall { broadcast_id: 1, original_epoch: epoch },
				FailedForeignChainCall { broadcast_id: 12, original_epoch: epoch },
			]
		);
		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch + 1),
			vec![FailedForeignChainCall { broadcast_id: 13, original_epoch: epoch }]
		);

		// Resign the 2nd call
		IngressEgress::on_finalize(2);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, ()>::FailedForeignChainCallResigned {
				broadcast_id: 12,
				threshold_signature_id: 3,
			},
		));
		assert_eq!(MockEgressBroadcaster::resigned_call(), Some(12u32));
		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch),
			vec![FailedForeignChainCall { broadcast_id: 1, original_epoch: epoch }]
		);
		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch + 1),
			vec![
				FailedForeignChainCall { broadcast_id: 13, original_epoch: epoch },
				FailedForeignChainCall { broadcast_id: 12, original_epoch: epoch }
			]
		);
		// Resign the last call
		IngressEgress::on_finalize(3);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, ()>::FailedForeignChainCallResigned {
				broadcast_id: 1,
				threshold_signature_id: 4,
			},
		));
		assert_eq!(MockEgressBroadcaster::resigned_call(), Some(1u32));
		assert_eq!(FailedForeignChainCalls::<Test, ()>::get(epoch), vec![]);
		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch + 1),
			vec![
				FailedForeignChainCall { broadcast_id: 13, original_epoch: epoch },
				FailedForeignChainCall { broadcast_id: 12, original_epoch: epoch },
				FailedForeignChainCall { broadcast_id: 1, original_epoch: epoch }
			]
		);

		// Failed calls are removed in the next epoch, 1 at a time.
		MockEpochInfo::set_epoch(epoch + 2);
		IngressEgress::on_finalize(4);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, ()>::FailedForeignChainCallExpired { broadcast_id: 1 },
		));
		assert_eq!(FailedForeignChainCalls::<Test, ()>::get(epoch), vec![]);
		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch + 1),
			vec![
				FailedForeignChainCall { broadcast_id: 13, original_epoch: epoch },
				FailedForeignChainCall { broadcast_id: 12, original_epoch: epoch }
			]
		);

		IngressEgress::on_finalize(5);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, ()>::FailedForeignChainCallExpired { broadcast_id: 12 },
		));
		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch + 1),
			vec![FailedForeignChainCall { broadcast_id: 13, original_epoch: epoch }]
		);

		IngressEgress::on_finalize(6);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			crate::Event::<Test, ()>::FailedForeignChainCallExpired { broadcast_id: 13 },
		));

		// All calls are culled from storage.
		assert!(!FailedForeignChainCalls::<Test, ()>::contains_key(epoch));
		assert!(!FailedForeignChainCalls::<Test, ()>::contains_key(epoch + 1));
		assert!(!FailedForeignChainCalls::<Test, ()>::contains_key(epoch + 2));
	});
}

#[test]
fn consolidation_tx_gets_broadcasted_on_finalize() {
	new_test_ext().execute_with(|| {
		// "Enable" consolidation for this test only to reduce noise in other tests
		cf_traits::mocks::api_call::SHOULD_CONSOLIDATE.with(|cell| cell.set(true));

		IngressEgress::on_finalize(1);

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(crate::Event::UtxoConsolidation {
			broadcast_id: 1,
		}));
	});
}

#[test]
fn all_batch_errors_are_logged_as_event() {
	new_test_ext()
		.execute_with(|| {
			ScheduledEgressFetchOrTransfer::<Test, ()>::set(vec![
				FetchOrTransfer::<Ethereum>::Transfer {
					asset: ETH_ETH,
					amount: 1_000,
					destination_address: ALICE_ETH_ADDRESS,
					egress_id: (ForeignChain::Ethereum, 1),
				},
			]);
			MockEthAllBatch::set_success(false);
		})
		.then_execute_at_next_block(|_| {})
		.then_execute_with(|_| {
			System::assert_last_event(RuntimeEvent::IngressEgress(
				crate::Event::<Test, ()>::FailedToBuildAllBatchCall {
					error: cf_chains::AllBatchError::UnsupportedToken,
				},
			));
		});
}

#[test]
fn broker_pays_a_fee_for_each_deposit_address() {
	new_test_ext().execute_with(|| {
		const CHANNEL_REQUESTER: u64 = 789;
		const FEE: u128 = 100;
		MockFundingInfo::<Test>::credit_funds(&CHANNEL_REQUESTER, FEE);
		assert_eq!(MockFundingInfo::<Test>::total_balance_of(&CHANNEL_REQUESTER), FEE);
		assert_ok!(IngressEgress::update_pallet_config(
			OriginTrait::root(),
			vec![PalletConfigUpdate::ChannelOpeningFee { fee: FEE }].try_into().unwrap()
		));
		assert_ok!(IngressEgress::open_channel(
			&CHANNEL_REQUESTER,
			EthAsset::Eth,
			ChannelAction::LiquidityProvision {
				lp_account: CHANNEL_REQUESTER,
				refund_address: Some(ForeignChainAddress::Eth(Default::default())),
			},
			0
		));
		assert_eq!(MockFundingInfo::<Test>::total_balance_of(&CHANNEL_REQUESTER), 0);
		assert_ok!(IngressEgress::update_pallet_config(
			OriginTrait::root(),
			vec![PalletConfigUpdate::ChannelOpeningFee { fee: FEE * 10 }]
				.try_into()
				.unwrap()
		));
		assert_err!(
			IngressEgress::open_channel(
				&CHANNEL_REQUESTER,
				EthAsset::Eth,
				ChannelAction::LiquidityProvision {
					lp_account: CHANNEL_REQUESTER,
					refund_address: Some(ForeignChainAddress::Eth(Default::default())),
				},
				0
			),
			mocks::fee_payment::ERROR_INSUFFICIENT_LIQUIDITY
		);
	});
}

#[test]
fn can_update_all_config_items() {
	new_test_ext().execute_with(|| {
		const NEW_OPENING_FEE: u128 = 300;
		const NEW_MIN_DEPOSIT_FLIP: u128 = 100;
		const NEW_MIN_DEPOSIT_ETH: u128 = 200;

		// Check that the default values are different from the new ones
		assert_eq!(ChannelOpeningFee::<Test, _>::get(), 0);
		assert_eq!(MinimumDeposit::<Test, _>::get(EthAsset::Flip), 0);
		assert_eq!(MinimumDeposit::<Test, _>::get(EthAsset::Eth), 0);

		// Update all config items at the same time, and updates 2 separate min deposit amounts.
		assert_ok!(IngressEgress::update_pallet_config(
			OriginTrait::root(),
			vec![
				PalletConfigUpdate::ChannelOpeningFee { fee: NEW_OPENING_FEE },
				PalletConfigUpdate::SetMinimumDeposit {
					asset: EthAsset::Flip,
					minimum_deposit: NEW_MIN_DEPOSIT_FLIP
				},
				PalletConfigUpdate::SetMinimumDeposit {
					asset: EthAsset::Eth,
					minimum_deposit: NEW_MIN_DEPOSIT_ETH
				},
			]
			.try_into()
			.unwrap()
		));

		// Check that the new values were set
		assert_eq!(ChannelOpeningFee::<Test, _>::get(), NEW_OPENING_FEE);
		assert_eq!(MinimumDeposit::<Test, _>::get(EthAsset::Flip), NEW_MIN_DEPOSIT_FLIP);
		assert_eq!(MinimumDeposit::<Test, _>::get(EthAsset::Eth), NEW_MIN_DEPOSIT_ETH);

		// Check that the events were emitted
		assert_events_eq!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::<Test, _>::ChannelOpeningFeeSet {
				fee: NEW_OPENING_FEE
			}),
			RuntimeEvent::IngressEgress(crate::Event::<Test, _>::MinimumDepositSet {
				asset: EthAsset::Flip,
				minimum_deposit: NEW_MIN_DEPOSIT_FLIP
			}),
			RuntimeEvent::IngressEgress(crate::Event::<Test, _>::MinimumDepositSet {
				asset: EthAsset::Eth,
				minimum_deposit: NEW_MIN_DEPOSIT_ETH
			}),
		);

		// Make sure that only governance can update the config
		assert_noop!(
			IngressEgress::update_pallet_config(
				OriginTrait::signed(ALICE),
				vec![].try_into().unwrap()
			),
			sp_runtime::traits::BadOrigin
		);
	});
}

fn test_ingress_or_egress_fee_is_withheld_or_scheduled_for_swap(test_function: impl Fn(EthAsset)) {
	new_test_ext().execute_with(|| {
		// Set the Gas (ingress egress Fee) via ChainTracker
		const GAS_FEE: u128 = DEFAULT_DEPOSIT_AMOUNT / 10;
		ChainTracker::<cf_chains::Ethereum>::set_fee(GAS_FEE);

		// Set the price of all non-gas assets to 1:1 with Eth so it makes the test easier
		MockAssetConverter::set_price(cf_primitives::Asset::Flip, cf_primitives::Asset::Eth, 1u128);
		MockAssetConverter::set_price(cf_primitives::Asset::Usdc, cf_primitives::Asset::Eth, 1u128);
		MockAssetConverter::set_price(cf_primitives::Asset::Usdt, cf_primitives::Asset::Eth, 1u128);

		// Should not schedule a swap because it is already the gas asset, but should withhold the
		// fee immediately.
		test_function(EthAsset::Eth);
		assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());

		assert_eq!(
			MockAssetWithholding::withheld_assets(ForeignChain::Ethereum.gas_asset()),
			GAS_FEE,
			"Expected ingress egress fee to be withheld for gas asset"
		);

		// All other assets should schedule a swap to the gas asset
		test_function(EthAsset::Flip);
		test_function(EthAsset::Usdc);
		test_function(EthAsset::Usdt);

		assert_eq!(
			MockSwapRequestHandler::<Test>::get_swap_requests(),
			vec![
				MockSwapRequest {
					input_asset: cf_primitives::Asset::Flip,
					output_asset: cf_primitives::Asset::Eth,
					input_amount: GAS_FEE,
					swap_type: SwapRequestType::IngressEgressFee,
					origin: SwapOrigin::Internal,
				},
				MockSwapRequest {
					input_asset: cf_primitives::Asset::Usdc,
					output_asset: cf_primitives::Asset::Eth,
					input_amount: GAS_FEE,
					swap_type: SwapRequestType::IngressEgressFee,
					origin: SwapOrigin::Internal,
				},
				MockSwapRequest {
					input_asset: cf_primitives::Asset::Usdt,
					output_asset: cf_primitives::Asset::Eth,
					input_amount: GAS_FEE,
					swap_type: SwapRequestType::IngressEgressFee,
					origin: SwapOrigin::Internal,
				}
			]
		);
	});
}

#[test]
fn egress_transaction_fee_is_withheld_or_scheduled_for_swap() {
	fn egress_function(asset: EthAsset) {
		<IngressEgress as EgressApi<Ethereum>>::schedule_egress(
			asset,
			DEFAULT_DEPOSIT_AMOUNT,
			Default::default(),
			None,
		)
		.unwrap();
	}

	test_ingress_or_egress_fee_is_withheld_or_scheduled_for_swap(egress_function)
}

#[test]
fn ingress_fee_is_withheld_or_scheduled_for_swap() {
	fn ingress_function(asset: EthAsset) {
		request_address_and_deposit(1u64, asset);
	}

	test_ingress_or_egress_fee_is_withheld_or_scheduled_for_swap(ingress_function)
}

#[test]
fn safe_mode_prevents_deposit_channel_creation() {
	new_test_ext().execute_with(|| {
		assert_ok!(IngressEgress::open_channel(
			&ALICE,
			EthAsset::Eth,
			ChannelAction::LiquidityProvision {
				lp_account: 0,
				refund_address: Some(ForeignChainAddress::Eth(Default::default()))
			},
			0,
		));

		use cf_traits::SetSafeMode;

		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			ingress_egress_ethereum: PalletSafeMode {
				deposits_enabled: false,
				..PalletSafeMode::CODE_GREEN
			},
		});

		assert_err!(
			IngressEgress::open_channel(
				&ALICE,
				EthAsset::Eth,
				ChannelAction::LiquidityProvision {
					lp_account: 0,
					refund_address: Some(ForeignChainAddress::Eth(Default::default()))
				},
				0,
			),
			crate::Error::<Test, _>::DepositChannelCreationDisabled
		);
	});
}

#[test]
fn only_governance_can_enable_or_disable_egress() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			IngressEgress::enable_or_disable_egress(OriginTrait::none(), ETH_ETH, true),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn do_not_batch_more_transfers_than_the_limit_allows() {
	new_test_ext().execute_with(|| {
		MockFetchesTransfersLimitProvider::enable_limits();

		const EXCESS_TRANSFERS: usize = 1;
		let transfer_limits = MockFetchesTransfersLimitProvider::maybe_transfers_limit().unwrap();

		for _ in 1..=transfer_limits + EXCESS_TRANSFERS {
			assert_ok!(IngressEgress::schedule_egress(ETH_ETH, 1_000, ALICE_ETH_ADDRESS, None));
		}

		let scheduled_egresses = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		assert_eq!(
			scheduled_egresses.len(),
			transfer_limits + 1,
			"Wrong amount of scheduled egresses!"
		);

		IngressEgress::on_finalize(1);

		let scheduled_egresses = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		assert_eq!(scheduled_egresses.len(), EXCESS_TRANSFERS, "Wrong amount of left egresses!");

		IngressEgress::on_finalize(2);

		let scheduled_egresses = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		assert_eq!(scheduled_egresses.len(), 0, "Left egresses have not been fully processed!");
	});
}

#[test]
fn do_not_batch_more_fetches_than_the_limit_allows() {
	new_test_ext().execute_with(|| {
		MockFetchesTransfersLimitProvider::enable_limits();

		const EXCESS_FETCHES: usize = 1;
		const ASSET: EthAsset = EthAsset::Eth;

		let fetch_limits = MockFetchesTransfersLimitProvider::maybe_fetches_limit().unwrap();

		for i in 1..=fetch_limits + EXCESS_FETCHES {
			let (_, address, ..) = IngressEgress::request_liquidity_deposit_address(
				i.try_into().unwrap(),
				ASSET,
				0,
				ForeignChainAddress::Eth(Default::default()),
			)
			.unwrap();
			let address: <Ethereum as Chain>::ChainAccount = address.try_into().unwrap();

			assert_ok!(IngressEgress::process_single_deposit(
				address,
				ASSET,
				DEFAULT_DEPOSIT_AMOUNT,
				Default::default(),
				Default::default()
			));
		}

		let scheduled_egresses = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		assert_eq!(
			scheduled_egresses.len(),
			fetch_limits + EXCESS_FETCHES,
			"Wrong amount of scheduled egresses!"
		);

		IngressEgress::on_finalize(1);

		let scheduled_egresses = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		assert_eq!(scheduled_egresses.len(), EXCESS_FETCHES, "Wrong amount of left egresses!");

		IngressEgress::on_finalize(2);

		let scheduled_egresses = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		assert_eq!(scheduled_egresses.len(), 0, "Left egresses have not been fully processed!");
	});
}

#[test]
fn do_not_process_more_ccm_swaps_than_allowed_by_limit() {
	new_test_ext().execute_with(|| {
		MockFetchesTransfersLimitProvider::enable_limits();

		const EXCESS_CCMS: usize = 1;
		let ccm_limits = MockFetchesTransfersLimitProvider::maybe_ccm_limit().unwrap();

		let gas_budget = 1000u128;
		let ccm = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
			channel_metadata: CcmChannelMetadata {
				message: vec![0x00, 0x01, 0x02].try_into().unwrap(),
				gas_budget: 1_000,
				ccm_additional_data: vec![].try_into().unwrap(),
			},
		};

		for _ in 1..=ccm_limits + EXCESS_CCMS {
			assert_ok!(IngressEgress::schedule_egress(
				ETH_ETH,
				1_000,
				ALICE_ETH_ADDRESS,
				Some((ccm.clone(), gas_budget))
			));
		}

		let scheduled_egresses = ScheduledEgressCcm::<Test, ()>::get();

		assert_eq!(
			scheduled_egresses.len(),
			ccm_limits + EXCESS_CCMS,
			"Wrong amount of scheduled egresses!"
		);

		IngressEgress::on_finalize(1);

		let scheduled_egresses = ScheduledEgressCcm::<Test, ()>::get();

		assert_eq!(scheduled_egresses.len(), EXCESS_CCMS, "Wrong amount of left egresses!");

		IngressEgress::on_finalize(2);

		let scheduled_egresses = ScheduledEgressCcm::<Test, ()>::get();

		assert_eq!(scheduled_egresses.len(), 0, "Left egresses have not been fully processed!");
	});
}

#[test]
fn can_request_swap_via_extrinsic() {
	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Flip;
	const INPUT_AMOUNT: AssetAmount = 1_000u128;

	const TX_HASH: [u8; 32] = [0xa; 32];

	let output_address = ForeignChainAddress::Eth([1; 20].into());

	new_test_ext().execute_with(|| {
		assert_ok!(IngressEgress::contract_swap_request(
			RuntimeOrigin::root(),
			INPUT_ASSET.try_into().unwrap(),
			OUTPUT_ASSET,
			INPUT_AMOUNT,
			MockAddressConverter::to_encoded_address(output_address.clone()),
			None,
			TX_HASH,
			Box::new(DepositDetails { tx_hashes: None }),
			Default::default(),
			None,
			None,
			0,
		));

		assert_eq!(
			MockSwapRequestHandler::<Test>::get_swap_requests(),
			vec![MockSwapRequest {
				input_asset: INPUT_ASSET,
				output_asset: OUTPUT_ASSET,
				input_amount: INPUT_AMOUNT,
				swap_type: SwapRequestType::Regular { output_address },
				origin: SwapOrigin::Vault { tx_hash: TX_HASH },
			},]
		);
	});
}

#[test]
fn can_request_ccm_swap_via_extrinsic() {
	const INPUT_ASSET: Asset = Asset::Flip;
	const OUTPUT_ASSET: Asset = Asset::Usdc;

	const INPUT_AMOUNT: AssetAmount = 10_000;
	const TX_HASH: [u8; 32] = [0xa; 32];

	let ccm_deposit_metadata = CcmDepositMetadata {
		source_chain: ForeignChain::Ethereum,
		source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
		channel_metadata: CcmChannelMetadata {
			message: vec![0x01].try_into().unwrap(),
			gas_budget: 1_000,
			ccm_additional_data: Default::default(),
		},
	};

	let output_address = ForeignChainAddress::Eth([1; 20].into());

	new_test_ext().execute_with(|| {
		assert_ok!(IngressEgress::contract_swap_request(
			RuntimeOrigin::root(),
			INPUT_ASSET.try_into().unwrap(),
			OUTPUT_ASSET,
			10_000,
			MockAddressConverter::to_encoded_address(output_address.clone()),
			Some(ccm_deposit_metadata.clone()),
			TX_HASH,
			Box::new(DepositDetails { tx_hashes: None }),
			Default::default(),
			None,
			None,
			0
		));

		assert_eq!(
			MockSwapRequestHandler::<Test>::get_swap_requests(),
			vec![MockSwapRequest {
				input_asset: INPUT_ASSET,
				output_asset: OUTPUT_ASSET,
				input_amount: INPUT_AMOUNT,
				swap_type: SwapRequestType::Ccm {
					output_address,
					ccm_swap_metadata: ccm_deposit_metadata
						.into_swap_metadata(INPUT_AMOUNT, INPUT_ASSET, OUTPUT_ASSET)
						.unwrap()
				},
				origin: SwapOrigin::Vault { tx_hash: TX_HASH },
			},]
		);
	});
}

#[test]
fn rejects_invalid_swap_by_witnesser() {
	new_test_ext().execute_with(|| {
		let script_pubkey = ScriptPubkey::try_from_address(
			"BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4",
			&BitcoinNetwork::Mainnet,
		)
		.unwrap();

		let btc_encoded_address =
			MockAddressConverter::to_encoded_address(ForeignChainAddress::Btc(script_pubkey));

		// Is valid Bitcoin address, but asset is Dot, so not compatible
		assert_ok!(IngressEgress::contract_swap_request(
			RuntimeOrigin::root(),
			EthAsset::Eth,
			Asset::Dot,
			10000,
			btc_encoded_address,
			None,
			Default::default(),
			Box::new(DepositDetails { tx_hashes: None }),
			Default::default(),
			None,
			None,
			0
		),);

		// No swap request created -> the call was ignored
		assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());

		// Invalid BTC address:
		assert_ok!(IngressEgress::contract_swap_request(
			RuntimeOrigin::root(),
			EthAsset::Eth,
			Asset::Btc,
			10000,
			EncodedAddress::Btc(vec![0x41, 0x80, 0x41]),
			None,
			Default::default(),
			Box::new(DepositDetails { tx_hashes: None }),
			Default::default(),
			None,
			None,
			0
		),);

		assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
	});
}

#[test]
fn failed_ccm_deposit_can_deposit_event() {
	const GAS_BUDGET: AssetAmount = 1_000;

	let ccm_deposit_metadata = CcmDepositMetadata {
		source_chain: ForeignChain::Ethereum,
		source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
		channel_metadata: CcmChannelMetadata {
			message: vec![0x01].try_into().unwrap(),
			gas_budget: GAS_BUDGET,
			ccm_additional_data: Default::default(),
		},
	};

	new_test_ext().execute_with(|| {
		// CCM is not supported for Dot:
		assert_ok!(IngressEgress::contract_swap_request(
			RuntimeOrigin::root(),
			EthAsset::Flip,
			Asset::Dot,
			10_000,
			EncodedAddress::Dot(Default::default()),
			Some(ccm_deposit_metadata.clone()),
			Default::default(),
			Box::new(DepositDetails { tx_hashes: None }),
			Default::default(),
			None,
			None,
			0
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::CcmFailed {
				reason: CcmFailReason::UnsupportedForTargetChain,
				..
			})
		);

		System::reset_events();

		// Insufficient deposit amount:
		assert_ok!(IngressEgress::contract_swap_request(
			RuntimeOrigin::root(),
			EthAsset::Flip,
			Asset::Eth,
			GAS_BUDGET - 1,
			EncodedAddress::Eth(Default::default()),
			Some(ccm_deposit_metadata),
			Default::default(),
			Box::new(DepositDetails { tx_hashes: None }),
			Default::default(),
			None,
			None,
			0
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::CcmFailed {
				reason: CcmFailReason::InsufficientDepositAmount,
				..
			})
		);
	});
}

#[test]
fn process_tainted_transaction_and_expect_refund() {
	new_test_ext().execute_with(|| {
		let (_, address) = request_address_and_deposit(BROKER, EthAsset::Eth);
		let _ = DepositChannelLookup::<Test, ()>::get(address).unwrap();

		let deposit_details: cf_chains::evm::DepositDetails = Default::default();

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));

		assert_ok!(IngressEgress::mark_transaction_as_tainted(
			OriginTrait::signed(BROKER),
			Default::default(),
		));

		assert_ok!(IngressEgress::process_single_deposit(
			address,
			EthAsset::Eth,
			DEFAULT_DEPOSIT_AMOUNT,
			deposit_details,
			Default::default()
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::<Test, ()>::DepositIgnored {
				deposit_address: _address,
				asset: EthAsset::Eth,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: _,
				reason: DepositIgnoredReason::TransactionTainted,
			})
		);

		assert_eq!(ScheduledTxForReject::<Test, ()>::decode_len(), Some(1));
	});
}

#[test]
fn only_broker_can_mark_transaction_as_tainted() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			IngressEgress::mark_transaction_as_tainted(
				OriginTrait::signed(ALICE),
				Default::default(),
			),
			BadOrigin
		);

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));

		assert_ok!(IngressEgress::mark_transaction_as_tainted(
			OriginTrait::signed(BROKER),
			Default::default(),
		));
	});
}

#[test]
fn tainted_transactions_expire_if_not_witnessed() {
	new_test_ext().execute_with(|| {
		let tx_id = DepositDetails::default();
		let expiry_at = System::block_number() + TAINTED_TX_EXPIRATION_BLOCKS as u64;

		let (_, address) = request_address_and_deposit(BROKER, EthAsset::Eth);
		let _ = DepositChannelLookup::<Test, ()>::get(address).unwrap();

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));

		assert_ok!(IngressEgress::mark_transaction_as_tainted(
			OriginTrait::signed(BROKER),
			Default::default(),
		));

		System::set_block_number(expiry_at);

		IngressEgress::on_idle(expiry_at, Weight::MAX);

		assert!(!TaintedTransactions::<Test, ()>::contains_key(BROKER, tx_id));

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(
			crate::Event::TaintedTransactionReportExpired {
				account_id: BROKER,
				tx_id: Default::default(),
			},
		));
	});
}

fn setup_boost_swap() -> ForeignChainAddress {
	assert_ok!(
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
			&ALICE,
		)
	);

	assert_ok!(IngressEgress::create_boost_pools(
		RuntimeOrigin::root(),
		vec![BoostPoolId { asset: EthAsset::Eth, tier: 10 }],
	));

	<Test as crate::Config>::Balance::try_credit_account(&ALICE, EthAsset::Eth.into(), 1000)
		.unwrap();

	let (_, address, _, _) = IngressEgress::request_swap_deposit_address(
		EthAsset::Eth,
		EthAsset::Eth.into(),
		ForeignChainAddress::Eth(Default::default()),
		Beneficiaries::new(),
		BROKER,
		None,
		10,
		None,
		None,
	)
	.unwrap();

	assert_ok!(IngressEgress::add_boost_funds(
		RuntimeOrigin::signed(ALICE),
		EthAsset::Eth,
		1000,
		10
	));

	address
}

#[test]
fn finalize_boosted_tx_if_tainted_after_prewitness() {
	new_test_ext().execute_with(|| {
		let tx_id = DepositDetails::default();

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));

		let address: <Ethereum as Chain>::ChainAccount = setup_boost_swap().try_into().unwrap();

		let _ = IngressEgress::add_prewitnessed_deposits(
			vec![DepositWitness {
				deposit_address: address,
				asset: EthAsset::Eth,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: tx_id.clone(),
			}],
			10,
		);

		assert_noop!(
			IngressEgress::mark_transaction_as_tainted(OriginTrait::signed(BROKER), tx_id.clone(),),
			crate::Error::<Test, ()>::TransactionAlreadyPrewitnessed
		);

		assert_ok!(IngressEgress::process_single_deposit(
			address,
			EthAsset::Eth,
			DEFAULT_DEPOSIT_AMOUNT,
			tx_id,
			Default::default()
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::DepositFinalised {
				deposit_address: _,
				asset: EthAsset::Eth,
				..
			})
		);
	});
}

#[test]
fn reject_tx_if_tainted_before_prewitness() {
	new_test_ext().execute_with(|| {
		let tx_id = DepositDetails::default();

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));

		let address: <Ethereum as Chain>::ChainAccount = setup_boost_swap().try_into().unwrap();

		assert_ok!(IngressEgress::mark_transaction_as_tainted(
			OriginTrait::signed(BROKER),
			tx_id.clone(),
		));

		let _ = IngressEgress::add_prewitnessed_deposits(
			vec![DepositWitness {
				deposit_address: address,
				asset: EthAsset::Eth,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: tx_id.clone(),
			}],
			10,
		);

		assert_ok!(IngressEgress::process_single_deposit(
			address,
			EthAsset::Eth,
			DEFAULT_DEPOSIT_AMOUNT,
			tx_id,
			Default::default()
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::DepositIgnored {
				deposit_address: _,
				asset: EthAsset::Eth,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: _,
				reason: DepositIgnoredReason::TransactionTainted,
			})
		);
	});
}

#[test]
fn do_not_expire_tainted_transactions_if_prewitnessed() {
	new_test_ext().execute_with(|| {
		let tx_id = DepositDetails::default();
		let expiry_at = System::block_number() + TAINTED_TX_EXPIRATION_BLOCKS as u64;

		TaintedTransactions::<Test, ()>::insert(
			BROKER,
			&tx_id,
			TaintedTransactionStatus::Prewitnessed,
		);

		ReportExpiresAt::<Test, ()>::insert(expiry_at, vec![(BROKER, tx_id.clone())]);

		IngressEgress::on_idle(expiry_at, Weight::MAX);

		assert!(TaintedTransactions::<Test, ()>::contains_key(BROKER, tx_id));
	});
}

#[test]
fn can_not_report_transaction_after_witnessing() {
	new_test_ext().execute_with(|| {
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));
		let unreported = evm::DepositDetails { tx_hashes: Some(vec![H256::random()]) };
		let unseen = evm::DepositDetails { tx_hashes: Some(vec![H256::random()]) };
		let prewitnessed = evm::DepositDetails { tx_hashes: Some(vec![H256::random()]) };
		let boosted = evm::DepositDetails { tx_hashes: Some(vec![H256::random()]) };

		TaintedTransactions::<Test, ()>::insert(BROKER, &unseen, TaintedTransactionStatus::Unseen);
		TaintedTransactions::<Test, ()>::insert(
			BROKER,
			&prewitnessed,
			TaintedTransactionStatus::Prewitnessed,
		);
		TaintedTransactions::<Test, ()>::insert(
			BROKER,
			&boosted,
			TaintedTransactionStatus::Boosted,
		);

		assert_ok!(IngressEgress::mark_transaction_as_tainted(
			OriginTrait::signed(BROKER),
			unreported,
		));
		assert_ok!(
			IngressEgress::mark_transaction_as_tainted(OriginTrait::signed(BROKER), unseen,)
		);
		assert_noop!(
			IngressEgress::mark_transaction_as_tainted(OriginTrait::signed(BROKER), prewitnessed,),
			crate::Error::<Test, ()>::TransactionAlreadyPrewitnessed
		);
		assert_noop!(
			IngressEgress::mark_transaction_as_tainted(OriginTrait::signed(BROKER), boosted,),
			crate::Error::<Test, ()>::TransactionAlreadyPrewitnessed
		);
	});
}
