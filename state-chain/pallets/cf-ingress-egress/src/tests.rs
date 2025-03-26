mod boost;
mod screening;

use crate::{
	mock_eth::*, BoostStatus, Call as PalletCall, ChannelAction, ChannelIdCounter,
	ChannelOpeningFee, CrossChainMessage, DepositAction, DepositChannelLifetime,
	DepositChannelLookup, DepositChannelPool, DepositFailedDetails, DepositFailedReason,
	DepositOrigin, DepositWitness, DisabledEgressAssets, EgressDustLimit, Event as PalletEvent,
	Event, FailedForeignChainCall, FailedForeignChainCalls, FetchOrTransfer, MinimumDeposit,
	NetworkFeeDeductionFromBoostPercent, Pallet, PalletConfigUpdate, PalletSafeMode,
	PrewitnessedDepositIdCounter, ScheduledEgressCcm, ScheduledEgressFetchOrTransfer,
	VaultDepositWitness, WitnessSafetyMargin,
};
use cf_chains::{
	address::{AddressConverter, EncodedAddress},
	assets::eth::Asset as EthAsset,
	btc::{BitcoinNetwork, ScriptPubkey},
	evm::{DepositDetails, EvmFetchId, H256},
	mocks::MockEthereum,
	CcmChannelMetadata, CcmDepositMetadata, Chain, ChannelRefundParameters, DepositChannel,
	DepositOriginType, ExecutexSwapAndCall, ForeignChainAddress, SwapOrigin,
	TransactionInIdForAnyChain, TransferAssetParams,
};

use cf_chains::eth::Address as EthereumAddress;

use crate::FailedRejections;
use cf_primitives::{
	AffiliateShortId, Affiliates, AssetAmount, BasisPoints, Beneficiaries, Beneficiary, ChannelId,
	DcaParameters, ForeignChain, MAX_AFFILIATES,
};
use cf_test_utilities::{assert_events_eq, assert_has_event, assert_has_matching_event};
use cf_traits::{
	mocks::{
		self,
		address_converter::MockAddressConverter,
		affiliate_registry::MockAffiliateRegistry,
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
	BalanceApi, DepositApi, EgressApi, EpochInfo, FetchesTransfersLimitProvider, FundingInfo,
	GetBlockHeight, SafeMode, ScheduledEgressDetails, SwapRequestType,
};
use frame_support::{
	assert_err, assert_noop, assert_ok,
	traits::{Hooks, OriginTrait},
	weights::Weight,
};
use sp_core::{bounded_vec, H160};
use sp_runtime::{DispatchError, DispatchResult, Percent};

const ALICE_ETH_ADDRESS: EthereumAddress = H160([100u8; 20]);
const BOB_ETH_ADDRESS: EthereumAddress = H160([101u8; 20]);
const ETH_ETH: EthAsset = EthAsset::Eth;
const ETH_FLIP: EthAsset = EthAsset::Flip;
const DEFAULT_DEPOSIT_AMOUNT: u128 = 1_000;
const ETH_REFUND_PARAMS: ChannelRefundParameters<H160> = ChannelRefundParameters {
	retry_duration: 0,
	refund_address: ALICE_ETH_ADDRESS,
	min_price: sp_core::U256::zero(),
};

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
		System::assert_last_event(RuntimeEvent::IngressEgress(Event::AssetEgressStatusChanged {
			asset,
			disabled: true,
		}));

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
		System::assert_last_event(RuntimeEvent::IngressEgress(Event::AssetEgressStatusChanged {
			asset,
			disabled: false,
		}));

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
			Some(ccm.clone()),
		));
		assert_ok!(IngressEgress::schedule_egress(
			ETH_FLIP,
			1_000,
			ALICE_ETH_ADDRESS,
			Some(ccm.clone()),
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
	assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
		&DepositWitness {
			deposit_address: address,
			asset,
			amount: DEFAULT_DEPOSIT_AMOUNT,
			deposit_details: Default::default()
		},
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

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(Event::DepositFetchesScheduled {
			channel_id: 1,
			asset: EthAsset::Eth,
		}));

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

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(Event::BatchBroadcastRequested {
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
		}));

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
			refund_address: ALICE_ETH_ADDRESS,
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
			Some(ccm.clone())
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
			// block height is purely informative.
			let block_height = BlockHeightProvider::<MockEthereum>::get_block_height();
			IngressEgress::process_channel_deposit_full_witness(
				DepositWitness {
					deposit_address: address,
					asset: ETH,
					amount: 1,
					deposit_details: Default::default(),
				},
				block_height,
			);

			IngressEgress::process_channel_deposit_full_witness(
				DepositWitness {
					deposit_address: address2,
					asset: ETH,
					amount: 1,
					deposit_details: Default::default(),
				},
				block_height,
			);
			(address, address2)
		})
		.then_process_events(|_, event| match event {
			RuntimeEvent::IngressEgress(Event::DepositFailed { .. }) |
			RuntimeEvent::IngressEgress(Event::DepositFinalised { .. }) => Some(event),
			_ => None,
		})
		.inspect_context(|((expected_rejected_address, expected_accepted_address), emitted)| {
			assert_eq!(emitted.len(), 2);
			assert!(emitted.iter().any(|e| matches!(
			e,
			RuntimeEvent::IngressEgress(
				Event::DepositFailed {
					details: DepositFailedDetails::DepositChannel { deposit_witness },
					..
				}) if deposit_witness.deposit_address == *expected_rejected_address
			)),);
			assert!(emitted.iter().any(|e| matches!(
			e,
			RuntimeEvent::IngressEgress(
				Event::DepositFinalised {
					deposit_address,
					..
				}) if deposit_address.as_ref() == Some(expected_accepted_address)
			)),);
		});
}

#[test]
fn multi_use_deposit_address_different_blocks() {
	const ETH: EthAsset = EthAsset::Eth;

	new_test_ext()
		.then_execute_at_next_block(|_| request_address_and_deposit(ALICE, ETH))
		.then_execute_at_next_block(|(_, deposit_address)| {
			assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
				&DepositWitness {
					deposit_address,
					asset: ETH,
					amount: 1,
					deposit_details: Default::default(),
				},
				// block height is purely informative.
				BlockHeightProvider::<MockEthereum>::get_block_height(),
			));
			deposit_address
		})
		.then_execute_at_next_block(|deposit_address| {
			assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
				&DepositWitness {
					deposit_address,
					asset: ETH,
					amount: 1,
					deposit_details: Default::default()
				},
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
			IngressEgress::process_channel_deposit_full_witness(
				DepositWitness {
					deposit_address,
					asset: ETH,
					amount: 1,
					deposit_details: Default::default(),
				},
				// block height is purely informative.
				BlockHeightProvider::<MockEthereum>::get_block_height(),
			);
			deposit_address
		})
		.then_process_events(|_, event| match event {
			RuntimeEvent::IngressEgress(Event::DepositFailed {
				details: DepositFailedDetails::DepositChannel { deposit_witness },
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
			let deposit_witness = DepositWitness {
				deposit_address,
				asset,
				amount: MinimumDeposit::<Test, ()>::get(asset) + DEPOSIT_AMOUNT,
				deposit_details: Default::default(),
			};
			assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
				&deposit_witness,
				Default::default(),
			));

			assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
				&deposit_witness,
				Default::default(),
			));

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
		let (_channel_id, deposit_address) = request_address_and_deposit(0, eth);
		System::assert_last_event(RuntimeEvent::IngressEgress(Event::DepositFailed {
			details: DepositFailedDetails::DepositChannel {
				deposit_witness: DepositWitness {
					deposit_address,
					asset: eth,
					amount: default_deposit_amount,
					deposit_details: Default::default(),
				},
			},
			reason: DepositFailedReason::BelowMinimumDeposit,
			block_height: Default::default(),
		}));

		const LP_ACCOUNT: u64 = 0;
		// Flip deposit should succeed.
		let (channel_id, deposit_address) = request_address_and_deposit(LP_ACCOUNT, flip);
		System::assert_last_event(RuntimeEvent::IngressEgress(Event::DepositFinalised {
			deposit_address: Some(deposit_address),
			asset: flip,
			amount: default_deposit_amount,
			block_height: Default::default(),
			deposit_details: Default::default(),
			ingress_fee: 0,
			max_boost_fee_bps: 0,
			action: DepositAction::LiquidityProvision { lp_account: LP_ACCOUNT },
			channel_id: Some(channel_id),
			origin_type: DepositOriginType::DepositChannel,
		}));
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
		let deposit = DepositWitness::<Ethereum> {
			deposit_address,
			asset: ASSET,
			amount: DEPOSIT_AMOUNT,
			deposit_details: Default::default(),
		};

		assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
			&deposit,
			Default::default()
		));
		// Observe the DepositFailed Event
		assert!(
			matches!(
				cf_test_utilities::last_event::<Test>(),
				RuntimeEvent::IngressEgress(Event::DepositFailed {
					reason: DepositFailedReason::NotEnoughToPayFees,
					..
				},)
			),
			"Expected DepositFailed Event, got: {:?}",
			cf_test_utilities::last_event::<Test>()
		);

		// Set fees to less than the deposit amount and retry.
		ChainTracker::<Ethereum>::set_fee(LOW_FEE);

		assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
			&deposit,
			Default::default()
		));
		// Observe the DepositReceived Event
		assert!(
			matches!(
				cf_test_utilities::last_event::<Test>(),
				RuntimeEvent::IngressEgress(Event::DepositFinalised {
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
		assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
			&DepositWitness {
				deposit_address,
				asset: ETH,
				amount: 1,
				deposit_details: Default::default(),
			},
			Default::default(),
		));
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
		assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
			&DepositWitness {
				deposit_address,
				asset: EthAsset::Eth,
				amount: 1,
				deposit_details: Default::default(),
			},
			Default::default(),
		));
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
				egress_details: None,
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
		System::assert_last_event(RuntimeEvent::IngressEgress(Event::CcmBroadcastFailed {
			broadcast_id,
		}));
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
			Event::FailedForeignChainCallResigned { broadcast_id: 13, threshold_signature_id: 2 },
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
			Event::FailedForeignChainCallResigned { broadcast_id: 12, threshold_signature_id: 3 },
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
			Event::FailedForeignChainCallResigned { broadcast_id: 1, threshold_signature_id: 4 },
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
			Event::FailedForeignChainCallExpired { broadcast_id: 1 },
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
			Event::FailedForeignChainCallExpired { broadcast_id: 12 },
		));
		assert_eq!(
			FailedForeignChainCalls::<Test, ()>::get(epoch + 1),
			vec![FailedForeignChainCall { broadcast_id: 13, original_epoch: epoch }]
		);

		IngressEgress::on_finalize(6);
		System::assert_last_event(RuntimeEvent::IngressEgress(
			Event::FailedForeignChainCallExpired { broadcast_id: 13 },
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

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(Event::UtxoConsolidation {
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
				Event::FailedToBuildAllBatchCall {
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
		const NEW_DEPOSIT_CHANNEL_LIFETIME: u64 = 99;
		const NETWORK_FEE_DEDUCTION: Percent = Percent::from_parts(50);
		const NEW_WITNESS_SAFETY_MARGIN: u64 = 300;
		// Check that the default values are different from the new ones
		assert_eq!(ChannelOpeningFee::<Test, _>::get(), 0);
		assert_eq!(MinimumDeposit::<Test, _>::get(EthAsset::Flip), 0);
		assert_eq!(MinimumDeposit::<Test, _>::get(EthAsset::Eth), 0);
		assert_ne!(DepositChannelLifetime::<Test, _>::get(), NEW_DEPOSIT_CHANNEL_LIFETIME);

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
				PalletConfigUpdate::SetDepositChannelLifetime {
					lifetime: NEW_DEPOSIT_CHANNEL_LIFETIME
				},
				PalletConfigUpdate::SetNetworkFeeDeductionFromBoost {
					deduction_percent: NETWORK_FEE_DEDUCTION
				},
				PalletConfigUpdate::SetWitnessSafetyMargin { margin: NEW_WITNESS_SAFETY_MARGIN }
			]
			.try_into()
			.unwrap()
		));

		// Check that the new values were set
		assert_eq!(ChannelOpeningFee::<Test, _>::get(), NEW_OPENING_FEE);
		assert_eq!(MinimumDeposit::<Test, _>::get(EthAsset::Flip), NEW_MIN_DEPOSIT_FLIP);
		assert_eq!(MinimumDeposit::<Test, _>::get(EthAsset::Eth), NEW_MIN_DEPOSIT_ETH);
		assert_eq!(DepositChannelLifetime::<Test, _>::get(), NEW_DEPOSIT_CHANNEL_LIFETIME);
		assert_eq!(NetworkFeeDeductionFromBoostPercent::<Test, _>::get(), NETWORK_FEE_DEDUCTION);
		assert_eq!(WitnessSafetyMargin::<Test, _>::get(), Some(NEW_WITNESS_SAFETY_MARGIN));

		// Check that the events were emitted
		assert_events_eq!(
			Test,
			RuntimeEvent::IngressEgress(Event::ChannelOpeningFeeSet { fee: NEW_OPENING_FEE }),
			RuntimeEvent::IngressEgress(Event::MinimumDepositSet {
				asset: EthAsset::Flip,
				minimum_deposit: NEW_MIN_DEPOSIT_FLIP
			}),
			RuntimeEvent::IngressEgress(Event::MinimumDepositSet {
				asset: EthAsset::Eth,
				minimum_deposit: NEW_MIN_DEPOSIT_ETH
			}),
			RuntimeEvent::IngressEgress(Event::DepositChannelLifetimeSet {
				lifetime: NEW_DEPOSIT_CHANNEL_LIFETIME
			}),
			RuntimeEvent::IngressEgress(Event::NetworkFeeDeductionFromBoostSet {
				deduction_percent: NETWORK_FEE_DEDUCTION
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
					broker_fees: Default::default(),
					origin: SwapOrigin::Internal,
				},
				MockSwapRequest {
					input_asset: cf_primitives::Asset::Usdc,
					output_asset: cf_primitives::Asset::Eth,
					input_amount: GAS_FEE,
					swap_type: SwapRequestType::IngressEgressFee,
					broker_fees: Default::default(),
					origin: SwapOrigin::Internal,
				},
				MockSwapRequest {
					input_asset: cf_primitives::Asset::Usdt,
					output_asset: cf_primitives::Asset::Eth,
					input_amount: GAS_FEE,
					swap_type: SwapRequestType::IngressEgressFee,
					broker_fees: Default::default(),
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

fn trigger_n_fetches(n: usize) -> Vec<H160> {
	let mut channel_addresses = vec![];

	const ASSET: EthAsset = EthAsset::Eth;

	for i in 1..=n {
		let (_, address, ..) = IngressEgress::request_liquidity_deposit_address(
			i.try_into().unwrap(),
			ASSET,
			0,
			ForeignChainAddress::Eth(Default::default()),
		)
		.unwrap();

		let address: <Ethereum as Chain>::ChainAccount = address.try_into().unwrap();

		channel_addresses.push(address);

		assert_ok!(IngressEgress::process_channel_deposit_full_witness_inner(
			&DepositWitness {
				deposit_address: address,
				asset: ASSET,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: Default::default(),
			},
			Default::default()
		));
	}

	channel_addresses
}

#[test]
fn do_not_batch_more_fetches_than_the_limit_allows() {
	new_test_ext().execute_with(|| {
		MockFetchesTransfersLimitProvider::enable_limits();

		const EXCESS_FETCHES: usize = 1;

		let fetch_limits = MockFetchesTransfersLimitProvider::maybe_fetches_limit().unwrap();

		trigger_n_fetches(fetch_limits + EXCESS_FETCHES);

		let scheduled_egresses = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		assert_eq!(
			scheduled_egresses.len(),
			fetch_limits + EXCESS_FETCHES,
			"Wrong amount of scheduled egresses!"
		);

		IngressEgress::on_finalize(1);

		let scheduled_egresses = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		// We should have fetched all except the exceess fetch.
		assert_eq!(scheduled_egresses.len(), EXCESS_FETCHES, "Wrong amount of left egresses!");

		IngressEgress::on_finalize(2);

		let scheduled_egresses = ScheduledEgressFetchOrTransfer::<Test, ()>::get();

		assert_eq!(scheduled_egresses.len(), 0, "Left egresses have not been fully processed!");
	});
}

#[test]
fn invalid_fetches_do_not_get_scheduled_and_do_not_block_other_fetches() {
	new_test_ext().execute_with(|| {
		MockFetchesTransfersLimitProvider::enable_limits();

		const EXCESS_FETCHES: usize = 5;

		let fetch_limits = MockFetchesTransfersLimitProvider::maybe_fetches_limit().unwrap();

		assert!(
			fetch_limits > EXCESS_FETCHES,
			"We assume excess_fetches can be processed in a single on_finalize for this test"
		);

		let channel_addresses = trigger_n_fetches(fetch_limits + EXCESS_FETCHES);

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, ()>::get().len(),
			fetch_limits + EXCESS_FETCHES,
			"All the fetches should have been scheduled!"
		);

		for address in channel_addresses.iter().take(fetch_limits) {
			IngressEgress::recycle_channel(&mut Weight::zero(), *address);
		}

		IngressEgress::on_finalize(1);

		// Check the addresses are the same as the expired ones, we can do this by comparing
		// the scheduled egresses with the expired addresses
		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, ()>::get()
				.iter()
				.filter_map(|f_or_t| match f_or_t {
					FetchOrTransfer::Fetch { deposit_address, .. } => Some(*deposit_address),
					_ => None,
				})
				.collect::<Vec<_>>(),
			channel_addresses[0..fetch_limits],
			// Note: Ideally this shouldn't be the case since we don't want to keep holding fetches
			// that will never be scheduled. However, at least we do not block ones that can be
			// scheduled.
			"The channels that expired should be the same as the scheduled egresses!"
		);
	});
}

#[test]
fn do_not_process_more_ccm_swaps_than_allowed_by_limit() {
	new_test_ext().execute_with(|| {
		MockFetchesTransfersLimitProvider::enable_limits();

		const EXCESS_CCMS: usize = 1;
		let ccm_limits = MockFetchesTransfersLimitProvider::maybe_ccm_limit().unwrap();

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
				Some(ccm.clone())
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

fn submit_vault_swap_request(
	input_asset: Asset,
	output_asset: Asset,
	deposit_amount: AssetAmount,
	deposit_address: H160,
	destination_address: EncodedAddress,
	deposit_metadata: Option<CcmDepositMetadata>,
	tx_id: H256,
	deposit_details: DepositDetails,
	broker_fee: Beneficiary<u64>,
	affiliate_fees: Affiliates<AffiliateShortId>,
	refund_params: ChannelRefundParameters<H160>,
	dca_params: Option<DcaParameters>,
	boost_fee: BasisPoints,
) -> DispatchResult {
	IngressEgress::vault_swap_request(
		RuntimeOrigin::root(),
		0,
		Box::new(VaultDepositWitness {
			input_asset: input_asset.try_into().unwrap(),
			deposit_address: Some(deposit_address),
			channel_id: Some(0),
			deposit_amount,
			deposit_details,
			output_asset,
			destination_address,
			deposit_metadata,
			tx_id,
			broker_fee: Some(broker_fee),
			affiliate_fees,
			refund_params,
			dca_params,
			boost_fee,
		}),
	)
}

#[test]
fn can_request_swap_via_extrinsic() {
	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Flip;
	const INPUT_AMOUNT: AssetAmount = 1_000u128;

	let output_address = ForeignChainAddress::Eth([1; 20].into());

	new_test_ext().execute_with(|| {
		assert_ok!(submit_vault_swap_request(
			INPUT_ASSET,
			OUTPUT_ASSET,
			INPUT_AMOUNT,
			Default::default(),
			MockAddressConverter::to_encoded_address(output_address.clone()),
			None,
			Default::default(),
			DepositDetails { tx_hashes: None },
			Beneficiary { account: BROKER, bps: 0 },
			Default::default(),
			ETH_REFUND_PARAMS,
			None,
			0
		));

		assert_eq!(
			MockSwapRequestHandler::<Test>::get_swap_requests(),
			vec![MockSwapRequest {
				input_asset: INPUT_ASSET,
				output_asset: OUTPUT_ASSET,
				input_amount: INPUT_AMOUNT,
				swap_type: SwapRequestType::Regular { output_address, ccm_deposit_metadata: None },
				broker_fees: bounded_vec![Beneficiary { account: BROKER, bps: 0 }],
				origin: SwapOrigin::Vault {
					tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
					broker_id: Some(BROKER),
				},
			},]
		);
	});
}

#[test]
fn vault_swaps_support_affiliate_fees() {
	new_test_ext().execute_with(|| {
		const INPUT_ASSET: Asset = Asset::Usdc;
		const OUTPUT_ASSET: Asset = Asset::Flip;
		const INPUT_AMOUNT: AssetAmount = 10_000;

		const BROKER_FEE: BasisPoints = 5;
		const AFFILIATE_FEE: BasisPoints = 10;
		const AFFILIATE_1: u64 = 102;
		const AFFILIATE_2: u64 = 103;

		const AFFILIATE_SHORT_1: AffiliateShortId = AffiliateShortId(0);
		const AFFILIATE_SHORT_2: AffiliateShortId = AffiliateShortId(1);

		let output_address = ForeignChainAddress::Eth([1; 20].into());

		// Register affiliate 1, but not affiliate 2 to check that we can
		// handle both cases:
		MockAffiliateRegistry::register_affiliate(BROKER, AFFILIATE_1, AFFILIATE_SHORT_1);
		// Note that another affiliate entries from different brokers don't overlap, so this should
		// have no effect on the test:
		MockAffiliateRegistry::register_affiliate(BROKER + 1, AFFILIATE_2, AFFILIATE_SHORT_1);

		assert_ok!(submit_vault_swap_request(
			INPUT_ASSET,
			OUTPUT_ASSET,
			INPUT_AMOUNT,
			Default::default(),
			MockAddressConverter::to_encoded_address(output_address.clone()),
			None,
			Default::default(),
			DepositDetails { tx_hashes: None },
			Beneficiary { account: BROKER, bps: BROKER_FEE },
			bounded_vec![
				Beneficiary { account: AFFILIATE_SHORT_1, bps: AFFILIATE_FEE },
				Beneficiary { account: AFFILIATE_SHORT_2, bps: AFFILIATE_FEE }
			],
			ETH_REFUND_PARAMS,
			None,
			0
		));

		assert_eq!(
			MockSwapRequestHandler::<Test>::get_swap_requests(),
			vec![MockSwapRequest {
				input_asset: INPUT_ASSET,
				output_asset: OUTPUT_ASSET,
				input_amount: INPUT_AMOUNT,
				swap_type: SwapRequestType::Regular { output_address, ccm_deposit_metadata: None },
				broker_fees: bounded_vec![
					Beneficiary { account: BROKER, bps: BROKER_FEE },
					// Only one affiliate is used (short id for affiliate 2 has not been
					// recognised):
					Beneficiary { account: AFFILIATE_1, bps: AFFILIATE_FEE }
				],
				origin: SwapOrigin::Vault {
					tx_id: cf_chains::TransactionInIdForAnyChain::Evm(H256::default()),
					broker_id: Some(BROKER),
				},
			},]
		);

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(PalletEvent::UnknownAffiliate {
			broker_id: BROKER,
			short_affiliate_id: AFFILIATE_SHORT_2,
		}));
	});
}

#[test]
fn charge_no_broker_fees_on_unknown_primary_broker() {
	new_test_ext().execute_with(|| {
		const INPUT_ASSET: Asset = Asset::Usdc;
		const OUTPUT_ASSET: Asset = Asset::Flip;
		const INPUT_AMOUNT: AssetAmount = 10_000;

		const BROKER_FEE: BasisPoints = 5;

		const NOT_A_BROKER: u64 = 357;

		let output_address = ForeignChainAddress::Eth([1; 20].into());

		assert_ok!(submit_vault_swap_request(
			INPUT_ASSET,
			OUTPUT_ASSET,
			INPUT_AMOUNT,
			Default::default(),
			MockAddressConverter::to_encoded_address(output_address.clone()),
			None,
			Default::default(),
			DepositDetails { tx_hashes: None },
			Beneficiary { account: NOT_A_BROKER, bps: BROKER_FEE },
			Default::default(),
			ETH_REFUND_PARAMS,
			None,
			0
		));

		// The request is recorded as not having any broker fees:
		assert_eq!(
			MockSwapRequestHandler::<Test>::get_swap_requests(),
			vec![MockSwapRequest {
				input_asset: INPUT_ASSET,
				output_asset: OUTPUT_ASSET,
				input_amount: INPUT_AMOUNT,
				swap_type: SwapRequestType::Regular { output_address, ccm_deposit_metadata: None },
				broker_fees: Default::default(),
				origin: SwapOrigin::Vault {
					tx_id: cf_chains::TransactionInIdForAnyChain::Evm(H256::default()),
					broker_id: Some(NOT_A_BROKER),
				},
			},]
		);

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(PalletEvent::UnknownBroker {
			broker_id: NOT_A_BROKER,
		}));
	});
}

#[test]
fn can_request_ccm_swap_via_extrinsic() {
	const INPUT_ASSET: Asset = Asset::Flip;
	const OUTPUT_ASSET: Asset = Asset::Usdc;

	const INPUT_AMOUNT: AssetAmount = 10_000;

	let ccm_deposit_metadata = CcmDepositMetadata {
		source_chain: ForeignChain::Ethereum,
		source_address: None,
		channel_metadata: CcmChannelMetadata {
			message: vec![0x01].try_into().unwrap(),
			gas_budget: 1_000,
			ccm_additional_data: Default::default(),
		},
	};

	let output_address = ForeignChainAddress::Eth([1; 20].into());

	new_test_ext().execute_with(|| {
		assert_ok!(submit_vault_swap_request(
			INPUT_ASSET,
			OUTPUT_ASSET,
			10_000,
			Default::default(),
			MockAddressConverter::to_encoded_address(output_address.clone()),
			Some(ccm_deposit_metadata.clone()),
			Default::default(),
			DepositDetails { tx_hashes: None },
			Beneficiary { account: BROKER, bps: 0 },
			Default::default(),
			ETH_REFUND_PARAMS,
			None,
			0
		));

		assert_eq!(
			MockSwapRequestHandler::<Test>::get_swap_requests(),
			vec![MockSwapRequest {
				input_asset: INPUT_ASSET,
				output_asset: OUTPUT_ASSET,
				input_amount: INPUT_AMOUNT,
				swap_type: SwapRequestType::Regular {
					output_address,
					ccm_deposit_metadata: Some(ccm_deposit_metadata)
				},
				broker_fees: bounded_vec![Beneficiary { account: BROKER, bps: 0 }],
				origin: SwapOrigin::Vault {
					tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
					broker_id: Some(BROKER),
				},
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
		assert_ok!(submit_vault_swap_request(
			Asset::Eth,
			Asset::Dot,
			10000,
			Default::default(),
			btc_encoded_address,
			None,
			Default::default(),
			DepositDetails { tx_hashes: None },
			Beneficiary { account: 0, bps: 0 },
			Default::default(),
			ETH_REFUND_PARAMS,
			None,
			0
		));

		// No swap request created -> the call was ignored
		assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());

		// Invalid BTC address:
		assert_ok!(submit_vault_swap_request(
			Asset::Eth,
			Asset::Btc,
			10000,
			Default::default(),
			EncodedAddress::Btc(vec![0x41, 0x80, 0x41]),
			None,
			Default::default(),
			DepositDetails { tx_hashes: None },
			Beneficiary { account: 0, bps: 0 },
			Default::default(),
			ETH_REFUND_PARAMS,
			None,
			0
		));

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
		assert_ok!(submit_vault_swap_request(
			Asset::Flip,
			Asset::Dot,
			10_000,
			Default::default(),
			EncodedAddress::Dot(Default::default()),
			Some(ccm_deposit_metadata.clone()),
			Default::default(),
			DepositDetails { tx_hashes: None },
			Beneficiary { account: 0, bps: 0 },
			Default::default(),
			ETH_REFUND_PARAMS,
			None,
			0
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(Event::DepositFailed {
				reason: DepositFailedReason::CcmUnsupportedForTargetChain,
				..
			})
		);

		System::reset_events();

		// Insufficient deposit amount:
		assert_ok!(submit_vault_swap_request(
			Asset::Flip,
			Asset::Eth,
			GAS_BUDGET - 1,
			Default::default(),
			EncodedAddress::Eth(Default::default()),
			Some(ccm_deposit_metadata),
			Default::default(),
			DepositDetails { tx_hashes: None },
			Beneficiary { account: 0, bps: 0 },
			Default::default(),
			ETH_REFUND_PARAMS,
			None,
			0
		));
	});
}

#[test]
fn private_and_regular_channel_ids_do_not_overlap() {
	new_test_ext().execute_with(|| {
		const REGULAR_CHANNEL_ID_1: u64 = 1;
		const PRIVATE_CHANNEL_ID: u64 = 2;
		const REGULAR_CHANNEL_ID_2: u64 = 3;

		let open_regular_channel_expecting_id = |expected_channel_id: u64| {
			let (channel_id, ..) = IngressEgress::open_channel(
				&ALICE,
				EthAsset::Eth,
				ChannelAction::LiquidityProvision {
					lp_account: 0,
					refund_address: Some(ForeignChainAddress::Eth(Default::default())),
				},
				0,
			)
			.unwrap();

			assert_eq!(channel_id, expected_channel_id);
		};

		// Open a regular channel first to check that ids of regular
		// and private channels do not overlap:
		open_regular_channel_expecting_id(REGULAR_CHANNEL_ID_1);

		// This method is used, for example, by the swapping pallet when requesting
		// a channel id for private broker channels:
		assert_eq!(IngressEgress::allocate_next_channel_id(), Ok(PRIVATE_CHANNEL_ID));

		// Open a regular channel again to check that opening a private channel
		// updates the channel id counter:
		open_regular_channel_expecting_id(REGULAR_CHANNEL_ID_2);
	});
}

#[test]
fn assembling_broker_fees() {
	new_test_ext().execute_with(|| {
		let broker_fee = Beneficiary { account: BROKER, bps: 0 };

		const AFFILIATE_IDS: [u64; 5] = [10, 20, 30, 40, 50];
		const AFFILIATE_SHORT_IDS: [u8; 5] = [1, 2, 3, 4, 5];

		assert_eq!(AFFILIATE_IDS.len(), MAX_AFFILIATES as usize);

		for (i, id) in AFFILIATE_IDS.into_iter().enumerate() {
			let short_id = AFFILIATE_SHORT_IDS[i];
			MockAffiliateRegistry::register_affiliate(BROKER, id, short_id.into());
		}

		let affiliate_fees: Vec<Beneficiary<AffiliateShortId>> = AFFILIATE_SHORT_IDS
			.into_iter()
			.map(|short_id| Beneficiary { account: short_id.into(), bps: short_id.into() })
			.collect();

		let affiliate_fees: Affiliates<AffiliateShortId> = affiliate_fees.try_into().unwrap();

		let expected: Beneficiaries<u64> = bounded_vec![
			Beneficiary { account: BROKER, bps: 0 },
			Beneficiary { account: 10, bps: 1 },
			Beneficiary { account: 20, bps: 2 },
			Beneficiary { account: 30, bps: 3 },
			Beneficiary { account: 40, bps: 4 },
			Beneficiary { account: 50, bps: 5 },
		];

		assert_eq!(IngressEgress::assemble_broker_fees(Some(broker_fee), affiliate_fees), expected);
	});
}

#[test]
fn ignore_change_of_minimum_deposit_if_deposit_is_not_boosted() {
	new_test_ext().execute_with(|| {
		const DEPOSIT_AMOUNT: AssetAmount = 100;

		// Increase the minimum deposit amount:
		MinimumDeposit::<Test, ()>::insert(EthAsset::Eth, DEPOSIT_AMOUNT + 1);

		assert_eq!(
			IngressEgress::process_full_witness_deposit_inner(
				None,
				Asset::Eth.try_into().unwrap(),
				DEPOSIT_AMOUNT,
				Default::default(),
				None,
				BoostStatus::NotBoosted,
				0,
				None,
				ChannelAction::LiquidityProvision {
					lp_account: 0,
					refund_address: Some(ForeignChainAddress::Eth(Default::default())),
				},
				0,
				DepositOrigin::Vault { tx_id: H256::default(), broker_id: Some(BROKER) },
			)
			.err(),
			Some(DepositFailedReason::BelowMinimumDeposit)
		);

		assert!(IngressEgress::process_full_witness_deposit_inner(
			None,
			Asset::Eth.try_into().unwrap(),
			DEPOSIT_AMOUNT,
			Default::default(),
			None,
			BoostStatus::Boosted {
				prewitnessed_deposit_id: 0,
				pools: vec![],
				amount: DEPOSIT_AMOUNT,
			},
			0,
			None,
			ChannelAction::LiquidityProvision {
				lp_account: 0,
				refund_address: Some(ForeignChainAddress::Eth(Default::default())),
			},
			0,
			DepositOrigin::Vault { tx_id: H256::default(), broker_id: Some(BROKER) },
		)
		.is_ok());
	});
}

#[cfg(test)]
mod evm_transaction_rejection {
	use super::*;
	use crate::{
		boost_pool, BoostPools, ScheduledTransactionsForRejection, TransactionRejectionDetails,
		TransactionRejectionStatus, TransactionsMarkedForRejection,
	};
	use cf_chains::{
		assets::eth::Asset as EthAsset, evm::H256, ChannelLifecycleHooks,
		DepositDetailsToTransactionInId,
	};
	use cf_traits::{
		mocks::account_role_registry::MockAccountRoleRegistry, AccountRoleRegistry, DepositApi,
	};
	use std::str::FromStr;

	const ETH: EthAsset = EthAsset::Eth;

	#[test]
	fn deposit_with_multiple_txs() {
		new_test_ext().execute_with(|| {
			let tx_ids = vec![
				H256::from_str(
					"0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
				)
				.unwrap(),
				H256::from_str(
					"0x3214567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
				)
				.unwrap(),
			];

			let (_, deposit_address, block, _) = IngressEgress::request_liquidity_deposit_address(
				BROKER,
				ETH,
				0,
				ForeignChainAddress::Eth(Default::default()),
			)
			.unwrap();

			assert_ok!(IngressEgress::mark_transaction_for_rejection(
				OriginTrait::signed(BROKER),
				tx_ids[0],
			));

			assert!(TransactionsMarkedForRejection::<Test, ()>::get(BROKER, tx_ids[0]).is_some());

			let deposit_details = DepositDetails { tx_hashes: Some(tx_ids.clone()) };

			let deposit_address: <Ethereum as Chain>::ChainAccount =
				deposit_address.try_into().unwrap();

			IngressEgress::process_channel_deposit_full_witness(
				DepositWitness {
					deposit_address,
					asset: ETH,
					amount: DEFAULT_DEPOSIT_AMOUNT,
					deposit_details: deposit_details.clone(),
				},
				block,
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::IngressEgress(crate::Event::<Test, ()>::DepositFailed {
					reason: DepositFailedReason::TransactionRejectedByBroker,
					details:DepositFailedDetails::DepositChannel {
						deposit_witness: DepositWitness {
							deposit_address: event_address,
							deposit_details: event_deposit_details,
							..
						}
					},
					..
				}) if *event_deposit_details == deposit_details && *event_address == deposit_address
			);

			assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());

			let scheduled_tx_for_reject = ScheduledTransactionsForRejection::<Test, ()>::get();
			assert_eq!(scheduled_tx_for_reject.len(), 1);

			IngressEgress::on_finalize(2);

			let scheduled_tx_for_reject = ScheduledTransactionsForRejection::<Test, ()>::get();
			assert_eq!(scheduled_tx_for_reject.len(), 0);
		});
	}

	#[test]
	fn deposit_with_single_tx() {
		new_test_ext().execute_with(|| {
			let tx_id = H256::from_str(
				"0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
			)
			.unwrap();
			let (_, deposit_address, block, _) = IngressEgress::request_liquidity_deposit_address(
				BROKER,
				ETH,
				0,
				ForeignChainAddress::Eth(Default::default()),
			)
			.unwrap();
			let deposit_address: <Ethereum as Chain>::ChainAccount =
				deposit_address.try_into().unwrap();
			let deposit_details = DepositDetails { tx_hashes: Some(vec![tx_id]) };
			// Report the tx as marked for rejection
			assert_ok!(IngressEgress::mark_transaction_for_rejection(
				OriginTrait::signed(BROKER),
				tx_id,
			));

			assert!(TransactionsMarkedForRejection::<Test, ()>::get(BROKER, tx_id).is_some());
			// Process the deposit
			IngressEgress::process_channel_deposit_full_witness(
				DepositWitness {
					deposit_address,
					asset: ETH,
					amount: DEFAULT_DEPOSIT_AMOUNT,
					deposit_details,
				},
				block,
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::IngressEgress(crate::Event::<Test, ()>::DepositFailed {
					reason: DepositFailedReason::TransactionRejectedByBroker,
					details:DepositFailedDetails::DepositChannel {
						deposit_witness: DepositWitness {
							deposit_details,
							..
						}
					},
					..
				}) if deposit_details.deposit_ids().unwrap().contains(&tx_id)
			);

			assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());

			let scheduled_tx_for_reject = ScheduledTransactionsForRejection::<Test, ()>::get();
			assert_eq!(scheduled_tx_for_reject.len(), 1);

			assert_eq!(
				scheduled_tx_for_reject[0].deposit_details.deposit_ids().unwrap(),
				vec![tx_id]
			);

			IngressEgress::on_finalize(2);

			let scheduled_tx_for_reject = ScheduledTransactionsForRejection::<Test, ()>::get();
			assert_eq!(scheduled_tx_for_reject.len(), 0);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::IngressEgress(
					crate::Event::<Test, ()>::TransactionRejectedByBroker { tx_id, .. }
				) if tx_id == tx_id
			);

			let pending_api_calls = MockEgressBroadcaster::get_pending_api_calls();

			assert_eq!(pending_api_calls.len(), 1);
			let api_call = pending_api_calls[0].clone();

			match api_call {
				MockEthereumApiCall::RejectCall { deposit_details, deposit_fetch_id, .. } => {
					assert_eq!(deposit_details.deposit_ids().unwrap(), vec![tx_id]);
					assert!(deposit_fetch_id.is_some());
				},
				_ => panic!("Expected a RejectCall"),
			}
		});
	}

	#[test]
	fn whitelisted_broker_can_mark_tx_for_rejection_for_lp() {
		new_test_ext().execute_with(|| {
			let tx_id = H256::from_str(
				"0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
			)
			.unwrap();

			assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
				&ALICE,
			));

			let (_, deposit_address, block, _) = IngressEgress::request_liquidity_deposit_address(
				ALICE,
				ETH,
				0,
				ForeignChainAddress::Eth(Default::default()),
			)
			.unwrap();

			let deposit_address: <Ethereum as Chain>::ChainAccount =
				deposit_address.try_into().unwrap();
			let deposit_details = DepositDetails { tx_hashes: Some(vec![tx_id]) };

			// Report the tx as marked for rejection
			assert_ok!(IngressEgress::mark_transaction_for_rejection(
				OriginTrait::signed(WHITELISTED_BROKER),
				tx_id,
			));
			assert!(TransactionsMarkedForRejection::<Test, ()>::get(SCREENING_ID, tx_id).is_some());

			// Process the deposit
			IngressEgress::process_channel_deposit_full_witness(
				DepositWitness {
					deposit_address,
					asset: ETH,
					amount: DEFAULT_DEPOSIT_AMOUNT,
					deposit_details,
				},
				block,
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::IngressEgress(crate::Event::<Test, ()>::DepositFailed {
					reason: DepositFailedReason::TransactionRejectedByBroker,
					details:DepositFailedDetails::DepositChannel {
						deposit_witness: DepositWitness {
							deposit_details,
							..
						}
					},
					..
				}) if deposit_details.deposit_ids().unwrap().contains(&tx_id)
			);

			assert!(TransactionsMarkedForRejection::<Test, ()>::get(SCREENING_ID, tx_id).is_none());

			assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
		});
	}

	#[test]
	fn whitelisted_broker_can_reject_two_concurrent_swap_deposits() {
		const TAINTED_TX_ID_1: H256 = H256::repeat_byte(0xaa);
		const TAINTED_TX_ID_2: H256 = H256::repeat_byte(0xbb);
		const COMMINGLED_TX_ID: H256 = H256::repeat_byte(0xcc);
		const CLEAN_TX_ID: H256 = H256::repeat_byte(0xdd);

		new_test_ext()
			.request_deposit_addresses(&[DepositRequest::SimpleSwap {
				source_asset: ETH_ETH,
				destination_asset: ETH_FLIP,
				destination_address: ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
				refund_address: ALICE_ETH_ADDRESS,
			}])
			.then_apply_extrinsics(|_| {
				[
					(
						OriginTrait::signed(WHITELISTED_BROKER),
						crate::Call::mark_transaction_for_rejection { tx_id: TAINTED_TX_ID_1 },
						Ok(()),
					),
					(
						OriginTrait::signed(WHITELISTED_BROKER),
						crate::Call::mark_transaction_for_rejection { tx_id: TAINTED_TX_ID_2 },
						Ok(()),
					),
				]
			})
			// we can't use `then_apply_extrinsics` because at the moment there's no way to
			// distinguish between pre-witness and witness origins.
			.then_execute_at_next_block(|deposits| {
				for (_, _, deposit_address) in &deposits {
					Pallet::<Test, _>::process_channel_deposit_full_witness(
						DepositWitness {
							deposit_address: *deposit_address,
							asset: ETH,
							amount: DEFAULT_DEPOSIT_AMOUNT,
							deposit_details: DepositDetails {
								tx_hashes: Some(vec![COMMINGLED_TX_ID, TAINTED_TX_ID_1]),
							},
						},
						100,
					);
				}
				deposits
			})
			.then_process_events(|_, event| match event {
				RuntimeEvent::IngressEgress(PalletEvent::DepositFetchesScheduled { .. }) => {
					panic!("Scheduled a fetch for a tainted tx");
				},
				RuntimeEvent::IngressEgress(PalletEvent::DepositFailed {
					reason: DepositFailedReason::TransactionRejectedByBroker,
					details:
						DepositFailedDetails::DepositChannel {
							deposit_witness: DepositWitness { deposit_details, .. },
						},
					..
				}) => {
					assert!(deposit_details.deposit_ids().unwrap().contains(&TAINTED_TX_ID_1));
					None
				},
				RuntimeEvent::IngressEgress(PalletEvent::TransactionRejectedByBroker {
					broadcast_id,
					tx_id,
				}) if tx_id.deposit_ids().unwrap().contains(&TAINTED_TX_ID_1) => Some(broadcast_id),
				_ => None,
			})
			.then_process_blocks(1)
			.then_execute_with(|(deposits, broadcast_ids)| {
				assert_eq!(broadcast_ids.len(), 1, "Expected 1 broadcast id");
				let _ = MockEgressBroadcaster::get_success_pending_callbacks()
					.pop()
					.expect("Expected a callback");

				MockEgressBroadcaster::get_pending_api_calls()
					.into_iter()
					.filter_map(|call| match call {
						MockEthereumApiCall::RejectCall { deposit_details, .. } =>
							Some(deposit_details.deposit_ids().unwrap()),
						_ => None,
					})
					.flatten()
					.find(|tx_id| *tx_id == TAINTED_TX_ID_1)
					.expect("Expected the tainted tx to be rejected");

				let (_, _, deposit_address) = deposits[0];
				let channel_details = DepositChannelLookup::<Test, _>::get(deposit_address)
					.expect("Channel ID should exist");
				assert!(
					!channel_details.deposit_channel.state.can_fetch(),
					"Channel should be pending and therefore unfetchable"
				);
				deposits
			})
			// Another deposit: we can't refund this one because the channel is pending and we can't
			// fetch the deposit.
			.then_execute_at_next_block(|deposits| {
				for (_, _, deposit_address) in &deposits {
					Pallet::<Test, _>::process_channel_deposit_full_witness(
						DepositWitness {
							deposit_address: *deposit_address,
							asset: ETH,
							amount: DEFAULT_DEPOSIT_AMOUNT,
							deposit_details: DepositDetails {
								tx_hashes: Some(vec![TAINTED_TX_ID_2]),
							},
						},
						100,
					);
				}
				deposits
			})
			.then_execute_at_next_block(|deposits| {
				for (_, _, deposit_address) in &deposits {
					Pallet::<Test, _>::process_channel_deposit_full_witness(
						DepositWitness {
							deposit_address: *deposit_address,
							asset: ETH,
							amount: DEFAULT_DEPOSIT_AMOUNT,
							deposit_details: DepositDetails { tx_hashes: Some(vec![CLEAN_TX_ID]) },
						},
						100,
					);
				}
				deposits
			})
			.then_execute_at_next_block(|deposits| {
				assert!(ScheduledTransactionsForRejection::<Test>::get().iter().any(
					|TransactionRejectionDetails { deposit_details, .. }| {
						deposit_details.deposit_ids().unwrap().contains(&TAINTED_TX_ID_2)
					}
				));
				deposits
			})
			// Still pending at next block.
			.then_execute_at_next_block(|deposits| {
				assert!(ScheduledTransactionsForRejection::<Test>::get().iter().any(
					|TransactionRejectionDetails { deposit_details, .. }| {
						deposit_details.deposit_ids().unwrap().contains(&TAINTED_TX_ID_2)
					}
				));
				deposits
			})
			// Simulate success -> apply success callbacks
			.then_apply_extrinsics(|_| {
				MockEgressBroadcaster::get_success_pending_callbacks()
					.iter()
					.map(|call| (OriginTrait::root(), call.clone(), Ok(())))
					.collect::<Vec<_>>()
			})
			.then_process_blocks(1)
			.then_execute_with_keep_context(|_| {
				assert!(
					ScheduledTransactionsForRejection::<Test>::get().is_empty(),
					"Expected no pending txs, but got {:#?}",
					ScheduledTransactionsForRejection::<Test>::get()
				);
				let rejected_ids = MockEgressBroadcaster::get_pending_api_calls()
					.into_iter()
					.filter_map(|call| match call {
						MockEthereumApiCall::RejectCall { deposit_details, .. } =>
							Some(deposit_details.deposit_ids().unwrap()),
						_ => None,
					})
					.flatten()
					.collect::<Vec<_>>();
				assert!(
					rejected_ids.contains(&TAINTED_TX_ID_1) &&
						rejected_ids.contains(&TAINTED_TX_ID_2) &&
						rejected_ids.contains(&COMMINGLED_TX_ID) &&
						!rejected_ids.contains(&CLEAN_TX_ID),
					"Expected the tainted and commingled txs to be rejected, but not the clean one."
				);
			});
	}

	#[test]
	fn mark_after_prewitness_has_no_effect() {
		const TAINTED_TX_ID: H256 = H256::repeat_byte(0xab);

		new_test_ext()
			// Add boost liquidity
			.then_execute_at_next_block(|_| {
				BoostPools::<Test, _>::insert(ETH_ETH, 10, {
					let mut pool = boost_pool::BoostPool::new(10);
					pool.add_funds(1234, 1_000_000);
					pool
				});
			})
			.request_deposit_addresses(&[DepositRequest::SimpleSwap {
				source_asset: ETH_ETH,
				destination_asset: ETH_FLIP,
				destination_address: ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
				refund_address: ALICE_ETH_ADDRESS,
			}])
			// Simulate a prewitness call.
			// we can't use `then_apply_extrinsics` because at the moment there's no way to
			// distinguish between pre-witness and witness origins.
			.then_execute_at_next_block(|deposits| {
				for (_, _, deposit_address) in &deposits {
					assert_ok!(Pallet::<Test, _>::process_channel_deposit_prewitness(
						DepositWitness::<Ethereum> {
							deposit_address: *deposit_address,
							asset: ETH_ETH,
							amount: 1_000_000,
							deposit_details: DepositDetails {
								tx_hashes: Some(vec![TAINTED_TX_ID])
							}
						},
						100,
					));
				}
				deposits
			})
			.then_apply_extrinsics(|_| {
				[(
					OriginTrait::signed(BROKER),
					crate::Call::mark_transaction_for_rejection { tx_id: TAINTED_TX_ID },
					Ok(()),
				)]
			})
			.then_execute_with_keep_context(|deposits| {
				assert!(TransactionsMarkedForRejection::<Test, ()>::get(
					SCREENING_ID,
					TAINTED_TX_ID
				)
				.is_none());
				assert!(
					TransactionsMarkedForRejection::<Test, ()>::get(SCREENING_ID, TAINTED_TX_ID)
						.is_none(),
					"Tx was not reported by whitelisted broker."
				);
				assert!(matches!(
					TransactionsMarkedForRejection::<Test, ()>::get(BROKER, TAINTED_TX_ID)
						.expect("Tx was marked by broker"),
					TransactionRejectionStatus { prewitnessed: false, .. }
				));
				assert!(
					matches!(
						DepositChannelLookup::<Test, _>::get(deposits[0].2)
							.expect("Deposit channel is not expired")
							.boost_status,
						BoostStatus::Boosted { .. },
					),
					"Deposit channel should be boosted, but is {:?}",
					DepositChannelLookup::<Test, _>::get(deposits[0].2).unwrap()
				);
			})
			.then_execute_at_next_block(|deposits| {
				for (_, _, deposit_address) in &deposits {
					Pallet::<Test, _>::process_channel_deposit_full_witness(
						DepositWitness {
							deposit_address: *deposit_address,
							asset: ETH,
							amount: DEFAULT_DEPOSIT_AMOUNT,
							deposit_details: DepositDetails {
								tx_hashes: Some(vec![TAINTED_TX_ID]),
							},
						},
						100,
					);
				}
				deposits
			})
			.then_process_events(|_, event| match event {
				RuntimeEvent::IngressEgress(PalletEvent::DepositFailed { .. }) => {
					panic!("Prewitnessed Deposit should not fail.");
				},
				RuntimeEvent::IngressEgress(PalletEvent::TransactionRejectedByBroker {
					..
				}) => panic!("Prewitnessed Transaction should not be rejected"),
				RuntimeEvent::IngressEgress(PalletEvent::DepositFetchesScheduled {
					channel_id,
					..
				}) => Some(channel_id),
				_ => None,
			})
			.inspect_context(|(deposits, scheduled_fetch_ids)| {
				assert_eq!(scheduled_fetch_ids.len(), 1, "Expected 1 fetch.");
				assert_eq!(
					*scheduled_fetch_ids,
					deposits.iter().map(|(_, id, _)| *id).collect::<Vec<_>>()
				);
			});
	}

	#[test]
	fn boosted_transaction_can_be_rejected_by_whitelisted_broker_or_owner() {
		const TAINTED_TX_ID_1: H256 = H256::repeat_byte(0xab);
		const TAINTED_TX_ID_2: H256 = H256::repeat_byte(0xcd);

		new_test_ext()
			// Add boost liquidity
			.then_execute_at_next_block(|_| {
				BoostPools::<Test, _>::insert(ETH_ETH, 10, {
					let mut pool = boost_pool::BoostPool::new(10);
					pool.add_funds(1234, 1_000_000);
					pool
				});
			})
			.request_deposit_addresses(&[DepositRequest::SimpleSwap {
				source_asset: ETH_ETH,
				destination_asset: ETH_FLIP,
				destination_address: ForeignChainAddress::Eth(ALICE_ETH_ADDRESS),
				refund_address: ALICE_ETH_ADDRESS,
			}])
			.assert_calls_ok(&[WHITELISTED_BROKER, BROKER][..], |id| {
				if *id == WHITELISTED_BROKER {
					crate::Call::mark_transaction_for_rejection { tx_id: TAINTED_TX_ID_1 }
				} else {
					crate::Call::mark_transaction_for_rejection { tx_id: TAINTED_TX_ID_2 }
				}
			})
			.then_execute_with_keep_context(|_| {
				assert!(TransactionsMarkedForRejection::<Test, ()>::get(
					SCREENING_ID,
					TAINTED_TX_ID_1
				)
				.is_some());
				assert!(TransactionsMarkedForRejection::<Test, ()>::get(BROKER, TAINTED_TX_ID_2)
					.is_some());
			})
			.then_execute_with_keep_context(|deposit_details| {
				let (_, _, deposit_address) = deposit_details[0];
				assert_ok!(Pallet::<Test, _>::process_deposits(
					OriginTrait::root(), // defaults to pre-witness origin
					[TAINTED_TX_ID_1, TAINTED_TX_ID_2]
						.into_iter()
						.map(|tx_id| DepositWitness {
							deposit_address,
							asset: ETH_ETH,
							amount: DEFAULT_DEPOSIT_AMOUNT,
							deposit_details: DepositDetails { tx_hashes: Some(vec![tx_id]) },
						})
						.collect::<Vec<_>>(),
					100,
				));
			})
			.then_process_events(|_, event| match event {
				RuntimeEvent::IngressEgress(PalletEvent::DepositBoosted { .. }) => {
					panic!("Boosted a fetch for a tainted tx");
				},
				_ => None::<()>,
			})
			.then_process_blocks(1)
			.then_execute_with_keep_context(|(deposit_details, _)| {
				let (_, _, deposit_address) = deposit_details[0];
				for tx_id in [TAINTED_TX_ID_1, TAINTED_TX_ID_2] {
					Pallet::<Test, _>::process_channel_deposit_full_witness(
						DepositWitness {
							deposit_address,
							asset: ETH_ETH,
							amount: DEFAULT_DEPOSIT_AMOUNT,
							deposit_details: DepositDetails { tx_hashes: Some(vec![tx_id]) },
						},
						100,
					);
				}
			})
			.then_process_events(|_, event| match event {
				RuntimeEvent::IngressEgress(PalletEvent::DepositFailed {
					reason: DepositFailedReason::TransactionRejectedByBroker,
					details:
						DepositFailedDetails::DepositChannel {
							deposit_witness: DepositWitness { deposit_details, .. },
						},
					..
				}) => Some(deposit_details.deposit_ids().unwrap().clone()),
				RuntimeEvent::IngressEgress(PalletEvent::DepositFetchesScheduled { .. }) |
				RuntimeEvent::IngressEgress(PalletEvent::DepositFinalised { .. }) => {
					panic!("Processed a deposit for a tainted tx");
				},
				_ => None,
			})
			.inspect_context(|(_, deposit_ids)| {
				let deposit_ids = deposit_ids.iter().flatten().collect::<Vec<_>>();
				assert_eq!(deposit_ids.len(), 2, "Expected 2 DepositIgnored events");
				assert!(deposit_ids.contains(&&TAINTED_TX_ID_1));
				assert!(deposit_ids.contains(&&TAINTED_TX_ID_2));
			});
	}

	#[test]
	fn rejecting_vault_swap() {
		let tx_id =
			H256::from_str("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef")
				.unwrap();

		new_test_ext()
			.then_execute_with(|_| {
				assert_ok!(IngressEgress::mark_transaction_for_rejection(
					OriginTrait::signed(BROKER),
					tx_id,
				));
			})
			.then_execute_at_next_block(|_| {
				assert_ok!(submit_vault_swap_request(
					Asset::Eth,
					Asset::Flip,
					DEFAULT_DEPOSIT_AMOUNT,
					Default::default(),
					MockAddressConverter::to_encoded_address(ForeignChainAddress::Eth(
						ALICE_ETH_ADDRESS
					)),
					None,
					tx_id,
					DepositDetails { tx_hashes: Some(vec![tx_id]) },
					Beneficiary { account: BROKER, bps: 0 },
					bounded_vec![],
					ETH_REFUND_PARAMS,
					None,
					0
				));
				assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());

				let scheduled_txs = ScheduledTransactionsForRejection::<Test, ()>::get();

				assert_eq!(scheduled_txs.len(), 1);
				assert_eq!(scheduled_txs[0].deposit_details.deposit_ids().unwrap(), vec![tx_id]);
			})
			.then_process_events(|_, event| match event {
				RuntimeEvent::IngressEgress(PalletEvent::TransactionRejectedByBroker {
					tx_id,
					..
				}) => Some(tx_id.tx_hashes.expect("Should not be empty.")),
				_ => None,
			})
			.then_execute_with(|(_, tx_hashes)| {
				assert_eq!(tx_hashes, vec![vec![tx_id]], "Expected exactly one rejection.");
				assert!(FailedRejections::<Test, ()>::get().is_empty());

				let pending_api_calls = MockEgressBroadcaster::get_pending_api_calls();

				assert_eq!(pending_api_calls.len(), 1);
				let api_call = pending_api_calls[0].clone();

				match api_call {
					MockEthereumApiCall::RejectCall {
						deposit_details, deposit_fetch_id, ..
					} => {
						assert_eq!(deposit_details.deposit_ids().unwrap(), vec![tx_id]);
						assert!(deposit_fetch_id.is_none());
					},
					_ => panic!("Expected a RejectCall"),
				}
			});
	}
}
