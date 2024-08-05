#![cfg(test)]

use super::*;
use cf_chains::{
	address::{AddressConverter, AddressDerivationApi, EncodedAddress},
	assets::any::Asset,
	ccm_checker::CcmValidityError,
	sol::{
		api::{SolanaEnvironment, SolanaTransactionBuildingError},
		sol_tx_core::sol_test_values,
		SolAddress, SolApiEnvironment, SolCcmAccounts, SolCcmAddress, SolHash, SolPubkey,
		SolTrackedData, SolanaCrypto,
	},
	CcmChannelMetadata, CcmDepositMetadata, Chain, ChainState, ExecutexSwapAndCallError,
	ForeignChainAddress, SetAggKeyWithAggKey, SetAggKeyWithAggKeyError, Solana, SwapOrigin,
};
use cf_primitives::{AccountRole, AuthorityCount, ForeignChain, SwapId};
use cf_test_utilities::{assert_events_match, assert_has_matching_event};
use codec::Encode;
use frame_support::traits::{OnFinalize, UnfilteredDispatchable};
use pallet_cf_ingress_egress::DepositWitness;
use pallet_cf_validator::RotationPhase;
use state_chain_runtime::{
	chainflip::{address_derivation::AddressDerivation, ChainAddressConverter, SolEnvironment},
	Runtime, RuntimeCall, RuntimeEvent, SolanaIngressEgress, SolanaInstance, SolanaThresholdSigner,
	Swapping,
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
const COMPUTE_PRICE: u64 = 1_000u64;

fn setup_sol_environments() {
	// Environment::SolanaApiEnvironment
	pallet_cf_environment::SolanaApiEnvironment::<Runtime>::set(SolApiEnvironment {
		vault_program: sol_test_values::VAULT_PROGRAM,
		vault_program_data_account: sol_test_values::VAULT_PROGRAM_DATA_ACCOUNT,
		token_vault_pda_account: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT,
		usdc_token_mint_pubkey: sol_test_values::USDC_TOKEN_MINT_PUB_KEY,
		usdc_token_vault_ata: sol_test_values::USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
	});

	// SolanaChainTracking::ChainState
	pallet_cf_chain_tracking::CurrentChainState::<Runtime, SolanaInstance>::set(Some(ChainState {
		block_height: 0,
		tracked_data: SolTrackedData { priority_fee: COMPUTE_PRICE },
	}));

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
) -> SwapId {
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
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
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

			testnet.move_to_the_next_epoch();

			// Initiate 2 swaps - Sol -> SolUsdc and SolUsdc -> Sol
			// This will results in 2 fetches and 2 transfers of different assets.
			assert_eq!(schedule_deposit_to_swap(ALICE, Asset::Sol, Asset::SolUsdc, None), 1);
			assert_eq!(schedule_deposit_to_swap(BOB, Asset::SolUsdc, Asset::Sol, None), 3);

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
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = network::fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_ok!(RuntimeCall::SolanaVault(pallet_cf_vaults::Call::<Runtime, SolanaInstance>::initialize_chain {})
				.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into())
			);
			setup_sol_environments();
			testnet.move_to_the_next_epoch();

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
		.blocks_per_epoch(EPOCH_BLOCKS)
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

			testnet.move_to_the_next_epoch();

			// Register 2 CCMs, one with Sol and one with SolUsdc token.
			assert_eq!(
				schedule_deposit_to_swap(
					ALICE,
					Asset::Sol,
					Asset::SolUsdc,
					Some(sol_test_values::ccm_parameter().channel_metadata)
				),
				1
			);
			assert_eq!(
				schedule_deposit_to_swap(
					BOB,
					Asset::SolUsdc,
					Asset::Sol,
					Some(sol_test_values::ccm_parameter().channel_metadata)
				),
				3
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
fn solana_ccm_fails_with_invalid_input() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
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

			testnet.move_to_the_next_epoch();

			let invalid_ccm = CcmDepositMetadata {
				source_chain: ForeignChain::Ethereum,
				source_address: Some(ForeignChainAddress::Eth([0xff; 20].into())),
				channel_metadata: CcmChannelMetadata {
					message: vec![0u8, 1u8, 2u8, 3u8].try_into().unwrap(),
					gas_budget: 0u128,
					cf_parameters: vec![0u8, 1u8, 2u8, 3u8].try_into().unwrap(),
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
			assert_noop!(
				RuntimeCall::Swapping(pallet_cf_swapping::Call::<Runtime>::ccm_deposit {
					source_asset: Asset::Sol,
					deposit_amount: 1_000_000_000_000u128,
					destination_asset: Asset::SolUsdc,
					destination_address: EncodedAddress::Sol([1u8; 32]),
					deposit_metadata: invalid_ccm,
					tx_hash: Default::default(),
				})
				.dispatch_bypass_filter(
					pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into()
				),
				pallet_cf_swapping::Error::<Runtime>::InvalidCcm,
			);

			// CCM building can still fail at building stage.
			let receiver = SolAddress([0xFF; 32]);
			let ccm = CcmDepositMetadata {
				source_chain: ForeignChain::Ethereum,
				source_address: Some(ForeignChainAddress::Eth([0xff; 20].into())),
				channel_metadata: CcmChannelMetadata {
					message: vec![0u8, 1u8, 2u8, 3u8].try_into().unwrap(),
					gas_budget: 0u128,
					cf_parameters: SolCcmAccounts {
						cf_receiver: SolCcmAddress { pubkey: receiver.into(), is_writable: true },
						remaining_accounts: vec![
							SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: false },
							SolCcmAddress { pubkey: SolPubkey([0x02; 32]), is_writable: false },
						],
					}
					.encode()
					.try_into()
					.unwrap(),
				},
			};

			witness_call(RuntimeCall::Swapping(pallet_cf_swapping::Call::<Runtime>::ccm_deposit {
				source_asset: Asset::Sol,
				deposit_amount: 1_000_000_000_000u128,
				destination_asset: Asset::SolUsdc,
				destination_address: EncodedAddress::Sol([1u8; 32]),
				deposit_metadata: ccm,
				tx_hash: Default::default(),
			}));
			// Setting the current agg key will invalidate the CCM.
			let epoch = SolanaThresholdSigner::current_key_epoch().unwrap();
			pallet_cf_threshold_signature::Keys::<Runtime, SolanaInstance>::set(
				epoch,
				Some(receiver),
			);

			let block_number = System::block_number() + pallet_cf_swapping::SWAP_DELAY_BLOCKS;
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
							CcmValidityError::CfParametersContainsInvalidAccounts
						)
					),
				}),
			);
		});
}

#[test]
fn failed_ccm_does_not_consume_durable_nonce() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
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

			testnet.move_to_the_next_epoch();

			let unavailable_nonces =
				pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter_keys()
					.count();

			// Failed Rotate Key message does not consume DurableNonce
			// Add extra Durable nonces to make RotateAggkey too long
			let available_nonces = (0..20)
				.map(|x| (SolAddress([x as u8; 32]), SolHash::default()))
				.collect::<Vec<_>>();
			pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::set(
				available_nonces.clone(),
			);
			assert_noop!(
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
