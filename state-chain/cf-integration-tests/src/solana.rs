#![cfg(test)]

use std::{collections::BTreeMap, marker::PhantomData};

use super::*;
use cf_chains::{
	address::{AddressConverter, AddressDerivationApi, EncodedAddress},
	assets::{any::Asset, sol::Asset as SolAsset},
	ccm_checker::{CcmValidityError, VersionedSolanaCcmAdditionalData},
	sol::{
		api::{SolanaApi, SolanaEnvironment, SolanaTransactionBuildingError},
		sol_tx_core::sol_test_values,
		transaction_builder::SolanaTransactionBuilder,
		SolAddress, SolAddressLookupTableAccount, SolApiEnvironment, SolCcmAccounts, SolCcmAddress,
		SolHash, SolPubkey, SolanaCrypto,
	},
	CcmChannelMetadata, CcmDepositMetadata, Chain, ChannelRefundParameters,
	ExecutexSwapAndCallError, ForeignChainAddress, RequiresSignatureRefresh, SetAggKeyWithAggKey,
	SetAggKeyWithAggKeyError, Solana, SwapOrigin, TransactionBuilder,
};
use cf_primitives::{AccountRole, AuthorityCount, ForeignChain, SwapRequestId};
use cf_test_utilities::{assert_events_match, assert_has_matching_event};
use cf_utilities::bs58_array;
use codec::Encode;
use frame_support::{
	assert_err,
	traits::{OnFinalize, UnfilteredDispatchable},
};
use pallet_cf_elections::{
	vote_storage::{composite::tuple_6_impls::CompositeVote, AuthorityVote},
	AuthorityVoteOf, ElectionIdentifierOf, MAXIMUM_VOTES_PER_EXTRINSIC,
};
use pallet_cf_ingress_egress::{
	DepositFailedReason, DepositWitness, FetchOrTransfer, VaultDepositWitness,
};
use pallet_cf_validator::RotationPhase;
use sp_core::ConstU32;
use sp_runtime::BoundedBTreeMap;
use state_chain_runtime::{
	chainflip::{
		address_derivation::AddressDerivation, solana_elections::TransactionSuccessDetails,
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
const REFUND_PARAMS: ChannelRefundParameters<SolAddress> = ChannelRefundParameters {
	retry_duration: 0,
	refund_address: FALLBACK_ADDRESS,
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
	pallet_cf_environment::SolanaApiEnvironment::<Runtime>::set(SolApiEnvironment {
		vault_program: sol_test_values::VAULT_PROGRAM,
		vault_program_data_account: sol_test_values::VAULT_PROGRAM_DATA_ACCOUNT,
		token_vault_pda_account: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT,
		usdc_token_mint_pubkey: sol_test_values::USDC_TOKEN_MINT_PUB_KEY,
		usdc_token_vault_ata: sol_test_values::USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
		swap_endpoint_program: sol_test_values::SWAP_ENDPOINT_PROGRAM,
		swap_endpoint_program_data_account: sol_test_values::SWAP_ENDPOINT_PROGRAM_DATA_ACCOUNT,
		alt_manager_program: sol_test_values::ALT_MANAGER_PROGRAM,
		address_lookup_table_account: SolAddressLookupTableAccount {
			key: sol_test_values::ADDRESS_LOOKUP_TABLE_ACCOUNT.into(),
			addresses: vec![],
		},
	});

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
	ccm: Option<CcmChannelMetadata>,
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
		None,
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
			assert!(matches!(
				Validator::current_rotation_phase(),
				RotationPhase::ActivatingKeys(..)
			));
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

fn vault_swap_deposit_witness(
	deposit_metadata: Option<CcmDepositMetadata>,
) -> VaultDepositWitness<Runtime, SolanaInstance> {
	VaultDepositWitness {
		input_asset: SolAsset::Sol,
		output_asset: Asset::SolUsdc,
		deposit_amount: 1_000_000_000_000u64,
		destination_address: EncodedAddress::Sol([1u8; 32]),
		deposit_metadata,
		tx_id: Default::default(),
		deposit_details: (),
		broker_fee: None,
		affiliate_fees: Default::default(),
		refund_params: Some(REFUND_PARAMS),
		dca_params: None,
		boost_fee: 0,
		deposit_address: Some(SolAddress([2u8; 32])),
		channel_id: Some(0),
	}
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
				channel_metadata: CcmChannelMetadata {
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
					None,
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
				),
				pallet_cf_swapping::Error::<Runtime>::InvalidCcm,
			);

			// Contract call fails with invalid CCM
			assert_ok!(RuntimeCall::SolanaIngressEgress(
				pallet_cf_ingress_egress::Call::vault_swap_request {
					block_height: 0,
					deposit: Box::new(vault_swap_deposit_witness(Some(invalid_ccm))),
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
				>::DepositFailed {
					reason: DepositFailedReason::CcmInvalidMetadata,
					..
				}),
			);

			System::reset_events();

			// CCM building can still fail at building stage.
			let receiver = SolAddress([0xFF; 32]);
			let ccm = CcmDepositMetadata {
				source_chain: ForeignChain::Ethereum,
				source_address: Some(ForeignChainAddress::Eth([0xff; 20].into())),
				channel_metadata: CcmChannelMetadata {
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
					deposit: Box::new(vault_swap_deposit_witness(Some(ccm))),
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
					egress_id: (ForeignChain::Solana, 1u64),
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
				channel_metadata: CcmChannelMetadata {
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
					deposit: Box::new(vault_swap_deposit_witness(Some(ccm))),
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
			assert!(matches!(pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::get()[0],
				FetchOrTransfer::Transfer {
					egress_id: (ForeignChain::Solana, 2),
					asset: SolAsset::SolUsdc,
					destination_address: FALLBACK_ADDRESS,
					..
				}
			));

			// Ensure the previous broadcast data has been cleaned up.
			assert!(!pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::get().contains(&ccm_broadcast_id));
			assert!(!pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::contains_key(ccm_broadcast_id));
			assert!(!pallet_cf_broadcast::TransactionOutIdToBroadcastId::<Runtime, SolanaInstance>::iter_values().any(|(broadcast_id, _)|broadcast_id == ccm_broadcast_id));
			assert!(!pallet_cf_broadcast::PendingApiCalls::<Runtime, SolanaInstance>::contains_key(ccm_broadcast_id));
		});
}
