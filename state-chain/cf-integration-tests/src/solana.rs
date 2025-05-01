// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

use std::{collections::BTreeMap, marker::PhantomData};

use super::*;
use cf_chains::{
	address::{AddressConverter, AddressDerivationApi, EncodedAddress},
	assets::{any::Asset, sol::Asset as SolAsset},
	ccm_checker::{CcmValidityError, DecodedCcmAdditionalData, VersionedSolanaCcmAdditionalData},
	sol::{
		api::{
			AltWitnessingConsensusResult, SolanaApi, SolanaEnvironment,
			SolanaTransactionBuildingError, SolanaTransactionType,
		},
		sol_tx_core::sol_test_values::{self, user_alt},
		transaction_builder::SolanaTransactionBuilder,
		SolAddress, SolAddressLookupTableAccount, SolCcmAccounts, SolCcmAddress, SolHash,
		SolPubkey, SolanaCrypto,
	},
	CcmChannelMetadata, CcmChannelMetadataUnchecked, CcmDepositMetadata, CcmDepositMetadataChecked,
	CcmDepositMetadataUnchecked, Chain, ChannelRefundParameters, ExecutexSwapAndCallError,
	ForeignChainAddress, RequiresSignatureRefresh, SetAggKeyWithAggKey, SetAggKeyWithAggKeyError,
	Solana, SwapOrigin, TransactionBuilder,
};
use cf_primitives::{AccountRole, AuthorityCount, Beneficiary, ForeignChain, SwapRequestId};
use cf_test_utilities::{assert_events_match, assert_has_matching_event};
use cf_traits::EgressApi;
use cf_utilities::{assert_matches, bs58_array};
use codec::Encode;
use frame_support::{
	assert_err,
	traits::{OnFinalize, UnfilteredDispatchable},
};
use pallet_cf_elections::{
	electoral_system::ElectoralReadAccess,
	electoral_systems::composite::tuple_7_impls::CompositeElectionIdentifierExtra,
	vote_storage::{composite::tuple_7_impls::CompositeVote, AuthorityVote},
	AuthorityVoteOf, ElectionIdentifier, ElectionIdentifierOf, UniqueMonotonicIdentifier,
	MAXIMUM_VOTES_PER_EXTRINSIC,
};

use pallet_cf_ingress_egress::{
	DepositAction, DepositWitness, FetchOrTransfer, RefundReason, VaultDepositWitness,
};
use pallet_cf_validator::RotationPhase;
use sp_core::ConstU32;
use sp_runtime::BoundedBTreeMap;
use state_chain_runtime::{
	chainflip::{
		address_derivation::AddressDerivation,
		solana_elections::{
			SolanaAltWitnessingElectoralAccess, SolanaAltWitnessingIdentifier,
			TransactionSuccessDetails,
		},
		ChainAddressConverter, SolEnvironment,
		SolanaTransactionBuilder as RuntimeSolanaTransactionBuilder,
	},
	Runtime, RuntimeCall, RuntimeEvent, SolanaElections, SolanaIngressEgress, SolanaInstance,
	SolanaThresholdSigner, Swapping,
};

use crate::{
	network::register_refund_addresses,
	swapping::{setup_pool_and_accounts, OrderType},
};

const DORIS: AccountId = AccountId::new([0x11; 32]);
const ZION: AccountId = AccountId::new([0x22; 32]);
const ALICE: AccountId = AccountId::new([0x33; 32]);
const BOB: AccountId = AccountId::new([0x44; 32]);

const DEPOSIT_AMOUNT: u64 = 5_000_000_000u64; // 5 Sol
const FALLBACK_ADDRESS: SolAddress = SolAddress([0xf0; 32]);
const REFUND_PARAMS: ChannelRefundParameters<EncodedAddress> = ChannelRefundParameters {
	retry_duration: 0,
	refund_address: EncodedAddress::Sol(FALLBACK_ADDRESS.0),
	min_price: sp_core::U256::zero(),
};

type SolanaElectionVote = BoundedBTreeMap<
	ElectionIdentifierOf<
		<Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner,
	>,
	AuthorityVoteOf<
		<Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner,
	>,
	ConstU32<MAXIMUM_VOTES_PER_EXTRINSIC>,
>;

fn setup_sol_environments() {
	// Environment::SolanaApiEnvironment
	pallet_cf_environment::SolanaApiEnvironment::<Runtime>::set(sol_test_values::api_env());

	// Environment::AvailableDurableNonces
	pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::set(
		sol_test_values::NONCE_ACCOUNTS
			.into_iter()
			.map(|nonce| (nonce, sol_test_values::TEST_DURABLE_NONCE))
			.collect::<Vec<_>>(),
	);
}

fn schedule_deposit_to_swap(
	who: AccountId,
	from: Asset,
	to: Asset,
	ccm: Option<CcmChannelMetadataUnchecked>,
) -> SwapRequestId {
	assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
		RuntimeOrigin::signed(who.clone()),
		from,
		to,
		EncodedAddress::Sol([1u8; 32]),
		0,
		ccm,
		0u16,
		Default::default(),
		REFUND_PARAMS,
		None,
	));

	let deposit_address = <AddressDerivation as AddressDerivationApi<Solana>>::generate_address(
		from.try_into().unwrap(),
		pallet_cf_ingress_egress::ChannelIdCounter::<Runtime, SolanaInstance>::get(),
	)
	.expect("Must be able to derive Solana deposit channel.");

	witness_call(RuntimeCall::SolanaIngressEgress(
		pallet_cf_ingress_egress::Call::process_deposits {
			deposit_witnesses: vec![DepositWitness {
				deposit_address,
				asset: from.try_into().unwrap(),
				amount: DEPOSIT_AMOUNT,
				deposit_details: Default::default(),
			}],
			block_height: 0,
		},
	));

	assert!(
		assert_events_match!(Runtime, RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapDepositAddressReady {
		deposit_address: event_deposit_address,
		source_asset,
		destination_asset,
		..
	}) if event_deposit_address == EncodedAddress::Sol(deposit_address.into())
		&& source_asset == from
		&& destination_asset == to
		=> true)
	);

	assert_events_match!(Runtime, RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequested {
		swap_request_id,
		origin: SwapOrigin::DepositChannel {
			deposit_address: events_deposit_address,
			..
		},
		..
	}) if <Solana as Chain>::ChainAccount::try_from(ChainAddressConverter::try_from_encoded_address(events_deposit_address.clone())
		.expect("we created the deposit address above so it should be valid")).unwrap() == deposit_address
		=> swap_request_id)
}

fn vote_for_alt_election(
	election_identifier: u64,
	res: AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>,
) {
	let mut vote = BoundedBTreeMap::new();
	vote.try_insert(
		ElectionIdentifier::new(
			UniqueMonotonicIdentifier::from(election_identifier),
			CompositeElectionIdentifierExtra::GG(()),
		),
		AuthorityVote::Vote(CompositeVote::GG(res)),
	)
	.unwrap();
	Validator::current_authorities().into_iter().for_each(|id| {
		assert_ok!(SolanaElections::stop_ignoring_my_votes(RuntimeOrigin::signed(id.clone()),));
		assert_ok!(RuntimeCall::SolanaElections(pallet_cf_elections::Call::<
			Runtime,
			SolanaInstance,
		>::vote {
			authority_votes: vote.clone()
		})
		.dispatch_bypass_filter(RuntimeOrigin::signed(id)));
	});
}

fn vault_swap_deposit_witness(
	broker: AccountId,
	deposit_metadata: Option<CcmDepositMetadataUnchecked<ForeignChainAddress>>,
) -> VaultDepositWitness<Runtime, SolanaInstance> {
	VaultDepositWitness {
		input_asset: SolAsset::Sol,
		output_asset: Asset::SolUsdc,
		deposit_amount: 1_000_000_000_000u64,
		destination_address: EncodedAddress::Sol([1u8; 32]),
		deposit_metadata,
		tx_id: Default::default(),
		deposit_details: (),
		broker_fee: Some(Beneficiary { account: broker, bps: 100u16 }),
		affiliate_fees: Default::default(),
		refund_params: ChannelRefundParameters {
			retry_duration: REFUND_PARAMS.retry_duration,
			refund_address: FALLBACK_ADDRESS,
			min_price: REFUND_PARAMS.min_price,
		},
		dca_params: None,
		boost_fee: 0,
		deposit_address: Some(SolAddress([2u8; 32])),
		channel_id: Some(0),
	}
}

#[test]
fn can_build_solana_batch_all() {
	const EPOCH_DURATION_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(ALICE, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(BOB, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			setup_sol_environments();

			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(
				pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {}
			)
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			register_refund_addresses(&DORIS);
			setup_pool_and_accounts(vec![Asset::Sol, Asset::SolUsdc], OrderType::LimitOrder);

			testnet.move_to_the_next_epoch();

			// Initiate 2 swaps - Sol -> SolUsdc and SolUsdc -> Sol
			// This will results in 2 fetches and 2 transfers of different assets.
			assert_eq!(schedule_deposit_to_swap(ALICE, Asset::Sol, Asset::SolUsdc, None), 1.into());
			assert_eq!(schedule_deposit_to_swap(BOB, Asset::SolUsdc, Asset::Sol, None), 3.into());

			// Verify the correct API call has been built, signed and broadcasted

			// Test that the BatchFetch is scheduled.
			testnet.move_forward_blocks(1);
			System::assert_has_event(
				RuntimeEvent::SolanaIngressEgress(pallet_cf_ingress_egress::Event::<
					Runtime,
					SolanaInstance,
				>::BatchBroadcastRequested {
					broadcast_id: 2,
					egress_ids: vec![],
				}),
			);

			testnet.move_forward_blocks(1);

			// Test that the 2 Transfers is scheduled.
			System::assert_has_event(
				RuntimeEvent::SolanaIngressEgress(pallet_cf_ingress_egress::Event::<
					Runtime,
					SolanaInstance,
				>::BatchBroadcastRequested {
					broadcast_id: 3,
					egress_ids: vec![(ForeignChain::Solana, 1)],
				}),
			);
			System::assert_has_event(
				RuntimeEvent::SolanaIngressEgress(pallet_cf_ingress_egress::Event::<
					Runtime,
					SolanaInstance,
				>::BatchBroadcastRequested {
					broadcast_id: 4,
					egress_ids: vec![(ForeignChain::Solana, 2)],
				}),
			);
		});
}

#[test]
fn can_rotate_solana_vault() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			setup_sol_environments();

			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {})
				.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into())
			);
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			assert_eq!(Validator::epoch_index(), 2);
			System::reset_events();

			let prev_key = <SolEnvironment as SolanaEnvironment>::current_agg_key().unwrap();

			// Move to when the new Vault Key is to be activated
			testnet.move_to_the_end_of_epoch();
			testnet.move_forward_blocks(10);

			// Assert the RotateKey call is built, signed and broadcasted.
			assert_matches!(
				Validator::current_rotation_phase(),
				RotationPhase::ActivatingKeys(..)
			);
			System::assert_has_event(RuntimeEvent::SolanaThresholdSigner(
				pallet_cf_threshold_signature::Event::<Runtime, SolanaInstance>::ThresholdSignatureSuccess {
					request_id: 3,
					ceremony_id: 5,
				})
			);
			assert!(assert_events_match!(Runtime, RuntimeEvent::SolanaBroadcaster(pallet_cf_broadcast::Event::<Runtime, SolanaInstance>::TransactionBroadcastRequest {
				broadcast_id,
				..
			}) if broadcast_id == 1 => true));
			System::assert_has_event(RuntimeEvent::SolanaThresholdSigner(
				pallet_cf_threshold_signature::Event::<Runtime, SolanaInstance>::ThresholdDispatchComplete {
					request_id: 3,
					ceremony_id: 5,
					result: Ok(()),
				})
			);

			// Complete the rotation.
			testnet.move_forward_blocks(2);
			assert_eq!(Validator::epoch_index(), 3);

			let new_key = <SolEnvironment as SolanaEnvironment>::current_agg_key().unwrap();
			assert!(prev_key != new_key);
		});
}

#[test]
fn can_send_solana_ccm() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(ALICE, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(BOB, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			setup_sol_environments();

			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(
				pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {}
			)
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			register_refund_addresses(&DORIS);
			setup_pool_and_accounts(vec![Asset::Sol, Asset::SolUsdc], OrderType::LimitOrder);

			testnet.move_to_the_next_epoch();

			// Register 2 CCMs, one with Sol and one with SolUsdc token.
			assert_eq!(
				schedule_deposit_to_swap(
					ALICE,
					Asset::Sol,
					Asset::SolUsdc,
					Some(sol_test_values::ccm_parameter().channel_metadata)
				),
				1.into()
			);
			assert_eq!(
				schedule_deposit_to_swap(
					BOB,
					Asset::SolUsdc,
					Asset::Sol,
					Some(sol_test_values::ccm_parameter().channel_metadata)
				),
				3.into()
			);

			// Wait until calls are built, signed and broadcasted.
			testnet.move_forward_blocks(1);
			System::assert_has_event(
				RuntimeEvent::SolanaIngressEgress(pallet_cf_ingress_egress::Event::<
					Runtime,
					SolanaInstance,
				>::BatchBroadcastRequested {
					broadcast_id: 2,
					egress_ids: vec![],
				}),
			);

			// 2 calls should be built - one for each CCM.
			testnet.move_forward_blocks(1);
			System::assert_has_event(RuntimeEvent::SolanaIngressEgress(
				pallet_cf_ingress_egress::Event::<Runtime, SolanaInstance>::CcmBroadcastRequested {
					broadcast_id: 3,
					egress_id: (ForeignChain::Solana, 1),
				},
			));
			System::assert_has_event(RuntimeEvent::SolanaIngressEgress(
				pallet_cf_ingress_egress::Event::<Runtime, SolanaInstance>::CcmBroadcastRequested {
					broadcast_id: 4,
					egress_id: (ForeignChain::Solana, 2),
				},
			));
		});
}

#[test]
fn can_send_solana_ccm_v1() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(ALICE, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(BOB, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			setup_sol_environments();

			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(
				pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {}
			)
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			register_refund_addresses(&DORIS);
			setup_pool_and_accounts(vec![Asset::Sol, Asset::SolUsdc], OrderType::LimitOrder);

			testnet.move_to_the_next_epoch();

			// Register 2 CCMs, one with Sol and one with SolUsdc token.
			assert_eq!(
				schedule_deposit_to_swap(
					ALICE,
					Asset::Sol,
					Asset::SolUsdc,
					Some(sol_test_values::ccm_parameter().channel_metadata)
				),
				1.into()
			);
			assert_eq!(
				schedule_deposit_to_swap(
					BOB,
					Asset::SolUsdc,
					Asset::Sol,
					Some(sol_test_values::ccm_parameter_v1().channel_metadata)
				),
				3.into()
			);

			testnet.move_forward_blocks(2);

			// CCM without ALT can be dispatched immediately
			System::assert_has_event(RuntimeEvent::SolanaIngressEgress(
				pallet_cf_ingress_egress::Event::<Runtime, SolanaInstance>::CcmBroadcastRequested {
					broadcast_id: 3,
					egress_id: (ForeignChain::Solana, 1),
				},
			));

			// Wait until swap is complete and ALT election started
			vote_for_alt_election(
				29,
				AltWitnessingConsensusResult::Valid(vec![SolAddressLookupTableAccount {
					key: user_alt().key,
					addresses: vec![Default::default()],
				}]),
			);

			// With consensus on ALT witnessing election, v1 CCM call is ready to be built.
			testnet.move_forward_blocks(1);

			System::assert_has_event(RuntimeEvent::SolanaIngressEgress(
				pallet_cf_ingress_egress::Event::<Runtime, SolanaInstance>::CcmBroadcastRequested {
					broadcast_id: 4,
					egress_id: (ForeignChain::Solana, 2),
				},
			));
		});
}

#[test]
fn ccms_can_contain_overlapping_and_identical_alts() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(ALICE, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(BOB, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			setup_sol_environments();

			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(
				pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {}
			)
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			register_refund_addresses(&DORIS);
			setup_pool_and_accounts(vec![Asset::Sol, Asset::SolUsdc], OrderType::LimitOrder);

			testnet.move_to_the_next_epoch();

			// Register 2 CCMs with overlapping ALTs
			let user_alts =
				[SolAddress([0xF0; 32]), SolAddress([0xF1; 32]), SolAddress([0xF2; 32])];
			let mut ccm_0 = sol_test_values::ccm_parameter().channel_metadata;
			ccm_0.ccm_additional_data =
				codec::Encode::encode(&VersionedSolanaCcmAdditionalData::V1 {
					ccm_accounts: sol_test_values::ccm_accounts(),
					alts: vec![user_alts[0], user_alts[1]],
				})
				.try_into()
				.unwrap();
			let mut ccm_1 = sol_test_values::ccm_parameter().channel_metadata;
			ccm_1.ccm_additional_data =
				codec::Encode::encode(&VersionedSolanaCcmAdditionalData::V1 {
					ccm_accounts: sol_test_values::ccm_accounts(),
					alts: vec![user_alts[1], user_alts[2]],
				})
				.try_into()
				.unwrap();

			assert_eq!(
				schedule_deposit_to_swap(ALICE, Asset::Sol, Asset::SolUsdc, Some(ccm_0.clone())),
				1.into()
			);
			assert_eq!(
				schedule_deposit_to_swap(BOB, Asset::SolUsdc, Asset::Sol, Some(ccm_1)),
				3.into()
			);
			assert_eq!(
				schedule_deposit_to_swap(ALICE, Asset::Sol, Asset::SolUsdc, Some(ccm_0)),
				4.into()
			);

			testnet.move_forward_blocks(2);

			// Let election come into Consensus
			vote_for_alt_election(
				29,
				AltWitnessingConsensusResult::Valid(vec![
					SolAddressLookupTableAccount {
						key: user_alts[0].into(),
						addresses: vec![SolPubkey([0xE0; 32]), SolPubkey([0xE1; 32])],
					},
					SolAddressLookupTableAccount {
						key: user_alts[1].into(),
						addresses: vec![SolPubkey([0xE2; 32]), SolPubkey([0xE3; 32])],
					},
				]),
			);
			vote_for_alt_election(
				30,
				AltWitnessingConsensusResult::Valid(vec![
					SolAddressLookupTableAccount {
						key: user_alts[1].into(),
						addresses: vec![SolPubkey([0xE2; 32]), SolPubkey([0xE3; 32])],
					},
					SolAddressLookupTableAccount {
						key: user_alts[2].into(),
						addresses: vec![SolPubkey([0xE4; 32]), SolPubkey([0xE5; 32])],
					},
				]),
			);
			vote_for_alt_election(
				31,
				AltWitnessingConsensusResult::Valid(vec![
					SolAddressLookupTableAccount {
						key: user_alts[0].into(),
						addresses: vec![SolPubkey([0xE0; 32]), SolPubkey([0xE1; 32])],
					},
					SolAddressLookupTableAccount {
						key: user_alts[1].into(),
						addresses: vec![SolPubkey([0xE2; 32]), SolPubkey([0xE3; 32])],
					},
				]),
			);
			testnet.move_forward_blocks(1);

			System::assert_has_event(RuntimeEvent::SolanaIngressEgress(
				pallet_cf_ingress_egress::Event::<Runtime, SolanaInstance>::CcmBroadcastRequested {
					broadcast_id: 3,
					egress_id: (ForeignChain::Solana, 1),
				},
			));
			System::assert_has_event(RuntimeEvent::SolanaIngressEgress(
				pallet_cf_ingress_egress::Event::<Runtime, SolanaInstance>::CcmBroadcastRequested {
					broadcast_id: 4,
					egress_id: (ForeignChain::Solana, 2),
				},
			));
			System::assert_has_event(RuntimeEvent::SolanaIngressEgress(
				pallet_cf_ingress_egress::Event::<Runtime, SolanaInstance>::CcmBroadcastRequested {
					broadcast_id: 5,
					egress_id: (ForeignChain::Solana, 3),
				},
			));

			// All CCMs are egressed successfully. Unsynchronised map states are consumed correctly.
			assert_eq!(
				pallet_cf_ingress_egress::ScheduledEgressCcm::<Runtime, SolanaInstance>::decode_len(
				),
				Some(0)
			);
			assert!(SolanaAltWitnessingElectoralAccess::unsynchronised_state_map(
				&SolanaAltWitnessingIdentifier(vec![user_alts[0], user_alts[1]])
			)
			.unwrap()
			.is_none());
			assert!(SolanaAltWitnessingElectoralAccess::unsynchronised_state_map(
				&SolanaAltWitnessingIdentifier(vec![user_alts[1], user_alts[2]])
			)
			.unwrap()
			.is_none());
		});
}

#[test]
fn solana_ccm_fails_with_invalid_input() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			setup_sol_environments();

			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(
				pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {}
			)
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			register_refund_addresses(&DORIS);
			setup_pool_and_accounts(vec![Asset::Sol, Asset::SolUsdc], OrderType::LimitOrder);

			testnet.move_to_the_next_epoch();

			let invalid_ccm = CcmDepositMetadata {
				source_chain: ForeignChain::Ethereum,
				source_address: Some(ForeignChainAddress::Eth([0xff; 20].into())),
				channel_metadata: CcmChannelMetadataUnchecked {
					message: vec![0u8, 1u8, 2u8, 3u8].try_into().unwrap(),
					gas_budget: 0u128,
					ccm_additional_data: vec![0u8, 1u8, 2u8, 3u8].try_into().unwrap(),
				},
			};

			// Unable to register a deposit channel using an invalid CCM
			assert_noop!(
				Swapping::request_swap_deposit_address_with_affiliates(
					RuntimeOrigin::signed(ZION),
					Asset::Sol,
					Asset::SolUsdc,
					EncodedAddress::Sol([1u8; 32]),
					0,
					Some(invalid_ccm.channel_metadata.clone()),
					0u16,
					Default::default(),
					REFUND_PARAMS,
					None,
				),
				pallet_cf_swapping::Error::<Runtime>::InvalidCcm,
			);
			assert_noop!(
				Swapping::request_swap_deposit_address(
					RuntimeOrigin::signed(ZION),
					Asset::Sol,
					Asset::SolUsdc,
					EncodedAddress::Sol([1u8; 32]),
					0,
					Some(invalid_ccm.channel_metadata.clone()),
					0u16,
					REFUND_PARAMS,
				),
				pallet_cf_swapping::Error::<Runtime>::InvalidCcm,
			);

			// Contract call fails with invalid CCM
			assert_ok!(RuntimeCall::SolanaIngressEgress(
				pallet_cf_ingress_egress::Call::vault_swap_request {
					block_height: 0,
					deposit: Box::new(vault_swap_deposit_witness(ZION, Some(invalid_ccm))),
				}
			)
			.dispatch_bypass_filter(
				pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into()
			),);

			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::SolanaIngressEgress(pallet_cf_ingress_egress::Event::<
					Runtime,
					SolanaInstance,
				>::DepositFinalised {
					action: DepositAction::Refund { reason: RefundReason::CcmInvalidMetadata, .. },
					..
				}),
			);

			System::reset_events();

			// CCM building can still fail at building stage.
			let receiver = SolAddress([0xFF; 32]);
			let ccm = CcmDepositMetadata {
				source_chain: ForeignChain::Ethereum,
				source_address: Some(ForeignChainAddress::Eth([0xff; 20].into())),
				channel_metadata: CcmChannelMetadataUnchecked {
					message: vec![0u8, 1u8, 2u8, 3u8].try_into().unwrap(),
					gas_budget: 0u128,
					ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
						cf_receiver: SolCcmAddress { pubkey: receiver.into(), is_writable: true },
						additional_accounts: vec![
							SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: false },
							SolCcmAddress { pubkey: SolPubkey([0x02; 32]), is_writable: false },
						],
						fallback_address: FALLBACK_ADDRESS.into(),
					})
					.encode()
					.try_into()
					.unwrap(),
				},
			};

			witness_call(RuntimeCall::SolanaIngressEgress(
				pallet_cf_ingress_egress::Call::vault_swap_request {
					block_height: 0,
					deposit: Box::new(vault_swap_deposit_witness(ZION, Some(ccm))),
				},
			));
			// Setting the current agg key will invalidate the CCM.
			let epoch = SolanaThresholdSigner::current_key_epoch().unwrap();
			pallet_cf_threshold_signature::Keys::<Runtime, SolanaInstance>::set(
				epoch,
				Some(receiver),
			);

			let block_number = System::block_number() + cf_primitives::SWAP_DELAY_BLOCKS;
			Swapping::on_finalize(block_number);
			SolanaIngressEgress::on_finalize(block_number);

			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::SolanaIngressEgress(pallet_cf_ingress_egress::Event::<
					Runtime,
					SolanaInstance,
				>::CcmEgressInvalid {
					egress_id: (ForeignChain::Solana, 2u64),
					error: ExecutexSwapAndCallError::FailedToBuildCcmForSolana(
						SolanaTransactionBuildingError::InvalidCcm(
							CcmValidityError::CcmAdditionalDataContainsInvalidAccounts
						)
					),
				}),
			);
		});
}

#[test]
fn failed_rotation_does_not_consume_durable_nonce() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			setup_sol_environments();

			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(
				pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {}
			)
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			register_refund_addresses(&DORIS);
			setup_pool_and_accounts(vec![Asset::Sol, Asset::SolUsdc], OrderType::LimitOrder);

			testnet.move_to_the_next_epoch();

			let unavailable_nonces =
				pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter_keys()
					.count();

			// Failed Rotate Key message does not consume DurableNonce
			// Add extra Durable nonces to make RotateAggkey too long
			let available_nonces = (0..100)
				.map(|x| (SolAddress([x as u8; 32]), SolHash::default()))
				.collect::<Vec<_>>();
			pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::set(
				available_nonces.clone(),
			);

			assert_err!(
				<cf_chains::sol::api::SolanaApi<SolEnvironment> as SetAggKeyWithAggKey<
					SolanaCrypto,
				>>::new_unsigned(None, SolAddress([0xff; 32]),),
				SetAggKeyWithAggKeyError::FinalTransactionExceededMaxLength
			);

			assert_eq!(
				available_nonces,
				pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::get()
			);
			assert_eq!(
				unavailable_nonces,
				pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter_keys()
					.count()
			)
		});
}

#[test]
fn solana_resigning() {
	use crate::solana::sol_test_values::TEST_DURABLE_NONCE;

	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(ALICE, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(BOB, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(
				pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {}
			)
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));
			testnet.move_to_the_next_epoch();

			setup_sol_environments();
			register_refund_addresses(&DORIS);
			setup_pool_and_accounts(vec![Asset::Sol, Asset::SolUsdc], OrderType::LimitOrder);

			const CURRENT_SIGNER: [u8; 32] = [3u8; 32];

			let mut transaction = SolanaTransactionBuilder::transfer_native(
				10000000,
				SolAddress(bs58_array("EwVmJgZwHBzmdVUzdujfbxFdaG25PMzbPLx8F7PvhWgs")),
				CURRENT_SIGNER.into(),
				(SolAddress(bs58_array("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw")), TEST_DURABLE_NONCE),
				100,
			).unwrap();
			transaction.signatures = vec![[1u8; 64].into()];

			let original_account_keys = transaction.message.static_account_keys();

			let apicall = SolanaApi {
				call_type: cf_chains::sol::api::SolanaTransactionType::Transfer,
				transaction: transaction.clone(),
				signer: Some(CURRENT_SIGNER.into()),
				_phantom: PhantomData::<SolEnvironment>,
			};

			let modified_call = RuntimeSolanaTransactionBuilder::requires_signature_refresh(
				&apicall,
				&Default::default(),
				Some([5u8; 32].into()),
			);
			if let RequiresSignatureRefresh::True(call) = modified_call {
				let agg_key = <SolEnvironment as SolanaEnvironment>::current_agg_key().unwrap();
				let transaction = call.clone().unwrap().transaction;
				for (modified_key, original_key) in transaction.message.static_account_keys().iter().zip(original_account_keys.iter()) {
					if *original_key != SolPubkey::from(CURRENT_SIGNER) {
						assert_eq!(modified_key, original_key);
						assert_ne!(*modified_key, SolPubkey::from(agg_key));
					} else {
						assert_eq!(*modified_key, SolPubkey::from(agg_key));
					}
				}
				let serialized_tx = transaction
					.clone()
					.finalize_and_serialize().unwrap();

				// Compare against a manually crafted transaction that works with the current test values and
				// agg_key. Not the signature itself
				let expected_serialized_tx = hex_literal::hex!("01000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008001000306f68d61e8d834034cf583f486f2a08ef53ce4134ed41c4d88f4720c39518745b617eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192cf1dd130e0341d60a0771ac40ea7900106a423354d2ecd6e609bd5e2ed833dec00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090380969800000000000400050284030000030200020c02000000809698000000000000").to_vec();

				assert_eq!(&serialized_tx[1+64..], &expected_serialized_tx[1+64..]);
				assert_eq!(&serialized_tx[0], &expected_serialized_tx[0]);
			} else {
				panic!("RequiresSignatureRefresh is False");
			}
		});
}

#[test]
fn solana_ccm_execution_error_can_trigger_fallback() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			setup_sol_environments();

			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(
				pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {}
			)
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));
			setup_pool_and_accounts(vec![Asset::Sol, Asset::SolUsdc], OrderType::LimitOrder);
			testnet.move_to_the_next_epoch();

			// Trigger a CCM swap
			let ccm = CcmDepositMetadata {
				source_chain: ForeignChain::Ethereum,
				source_address: Some(ForeignChainAddress::Eth([0xff; 20].into())),
				channel_metadata: CcmChannelMetadataUnchecked {
					message: vec![0u8, 1u8, 2u8, 3u8].try_into().unwrap(),
					gas_budget: 1_000_000_000u128,
					ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
						cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x10; 32]), is_writable: true },
						additional_accounts: vec![
							SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: false },
							SolCcmAddress { pubkey: SolPubkey([0x02; 32]), is_writable: false },
						],
						fallback_address: FALLBACK_ADDRESS.into(),
					})
					.encode()
					.try_into()
					.unwrap(),
				},
			};
			witness_call(RuntimeCall::SolanaIngressEgress(
				pallet_cf_ingress_egress::Call::vault_swap_request {
					block_height: 0,
					deposit: Box::new(vault_swap_deposit_witness(ZION, Some(ccm))),
				}
			));

			// Wait for the swaps to complete and call broadcasted.
			testnet.move_forward_blocks(5);

			// Get the broadcast ID for the ccm. There should be only one broadcast pending.
			assert_eq!(pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::get().len(), 1);
			let ccm_broadcast_id = pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::get().into_iter().next().unwrap();

			// Get the election identifier of the Solana egress.
			let election_id = SolanaElections::with_election_identifiers(
				|election_identifiers| {
					Ok(election_identifiers.last().cloned().unwrap())
				},
			).unwrap();

			// Submit vote to witness: transaction success, but execution failure
			let vote: SolanaElectionVote = BTreeMap::from_iter([(election_id,
				AuthorityVote::Vote(CompositeVote::D(TransactionSuccessDetails {
					tx_fee: 1_000,
					transaction_successful: false,
				}))
			)]).try_into().unwrap();

			for v in Validator::current_authorities() {
				assert_ok!(SolanaElections::stop_ignoring_my_votes(
					RuntimeOrigin::signed(v.clone()),
				));

				assert_ok!(SolanaElections::vote(
					RuntimeOrigin::signed(v),
					vote.clone()
				));
			}

			// Egress queue should be empty
			assert_eq!(pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::decode_len(), Some(0));

			// on_finalize: reach consensus on the egress vote and trigger the fallback mechanism.
			SolanaElections::on_finalize(System::block_number() + 1);
			assert_eq!(pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::decode_len(), Some(1));
			assert_matches!(pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::get()[0],
				FetchOrTransfer::Transfer {
					egress_id: (ForeignChain::Solana, 2),
					asset: SolAsset::SolUsdc,
					destination_address: FALLBACK_ADDRESS,
					..
				}
			);

			// Ensure the previous broadcast data has been cleaned up.
			assert!(!pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::get().contains(&ccm_broadcast_id));
			assert!(!pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::contains_key(ccm_broadcast_id));
			assert!(!pallet_cf_broadcast::TransactionOutIdToBroadcastId::<Runtime, SolanaInstance>::iter_values().any(|(broadcast_id, _)|broadcast_id == ccm_broadcast_id));
			assert!(!pallet_cf_broadcast::PendingApiCalls::<Runtime, SolanaInstance>::contains_key(ccm_broadcast_id));
		});
}

#[test]
fn invalid_alt_triggers_refund_transfer() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			setup_sol_environments();

			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(
				pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {}
			)
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));
			setup_pool_and_accounts(vec![Asset::Sol, Asset::SolUsdc], OrderType::LimitOrder);
			testnet.move_to_the_next_epoch();

			let destination_address = SolAddress([0xcf; 32]);
			let alt_0 = SolAddress([0xF0; 32]);
			let alt_1 = SolAddress([0xF1; 32]);

			// Directly insert a CCM to be ingressed.
			assert_ok!(SolanaIngressEgress::schedule_egress(
				SolAsset::Sol,
				1_000_000_000_000u64,
				destination_address,
				Some(CcmDepositMetadataChecked {
					channel_metadata: CcmChannelMetadata {
						message: vec![0u8, 1u8, 2u8, 3u8].try_into().unwrap(),
						gas_budget: 1_000_000_000u128,
						ccm_additional_data: DecodedCcmAdditionalData::Solana(
							VersionedSolanaCcmAdditionalData::V1 {
								ccm_accounts: SolCcmAccounts {
									cf_receiver: SolCcmAddress {
										pubkey: SolPubkey([0x10; 32]),
										is_writable: true,
									},
									additional_accounts: vec![SolCcmAddress {
										pubkey: SolPubkey([0x01; 32]),
										is_writable: false,
									}],
									fallback_address: FALLBACK_ADDRESS.into(),
								},
								alts: vec![alt_0, alt_1],
							},
						),
					},
					source_chain: ForeignChain::Ethereum,
					source_address: None,
				}),
			));

			testnet.move_forward_blocks(1);

			vote_for_alt_election(13, AltWitnessingConsensusResult::Invalid);

			// Let the election come to consensus.
			testnet.move_forward_blocks(1);

			// When CCM transaction building failed, fallback to refund the asset via Transfer
			// instead.
			assert!(assert_events_match!(
				Runtime,
				RuntimeEvent::SolanaIngressEgress(
					pallet_cf_ingress_egress::Event::<Runtime, SolanaInstance>::InvalidCcmRefunded {
						asset,
						destination_address,
						..
					}) if asset == SolAsset::Sol && destination_address == FALLBACK_ADDRESS => true
			));

			// Give enough time to schedule, egress and threshold-sign the transfer transaction.
			testnet.move_forward_blocks(4);
			let broadcast_id =
				pallet_cf_broadcast::BroadcastIdCounter::<Runtime, SolanaInstance>::get();

			// Transfer transaction should be created against the refund address.
			assert!(pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::get()
				.contains(&broadcast_id));
			assert!(matches!(
				pallet_cf_broadcast::PendingApiCalls::<Runtime, SolanaInstance>::get(broadcast_id),
				Some(SolanaApi { call_type: SolanaTransactionType::Transfer, .. })
			));
		});
}
