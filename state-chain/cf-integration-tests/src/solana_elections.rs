#![cfg(test)]

use crate::swapping::{setup_pool_and_accounts, OrderType};
use cf_chains::{
	address::{AddressDerivationApi, EncodedAddress},
	assets::sol::Asset as SolAsset,
	sol::{api::SolanaEnvironment, SolHash},
	Chain, Solana,
};
use cf_primitives::{AccountRole, Asset};
use pallet_cf_elections::electoral_systems::blockchain::delta_based_ingress::ChannelTotalIngressedFor;
use state_chain_runtime::{
	chainflip::{
		address_derivation::AddressDerivation, solana_elections::SolanaChainTrackingProvider,
		SolEnvironment,
	},
	Runtime, RuntimeEvent, RuntimeOrigin, SolanaChainTracking, SolanaIngressEgress, SolanaInstance,
	Swapping,
};

use super::*;
use crate::solana_test_utils::*;
use cf_test_utilities::assert_events_match;
use frame_support::traits::UnfilteredDispatchable;

const DORIS: AccountId = AccountId::new([0xE1; 32]);
const ZION: AccountId = AccountId::new([0xE2; 32]);
const EPOCH_BLOCKS: u32 = 100;

#[test]
fn solana_block_height_tracking_works() {
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
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

			assert_eq!(SolanaChainTracking::chain_state().unwrap().block_height, 0);

			const NEW_BLOCK_HEIGHT: u64 = 100u64;

			witness_solana_state(SolanaState::BlockHeight(NEW_BLOCK_HEIGHT));

			testnet.move_forward_blocks(1);

			assert_eq!(SolanaChainTracking::chain_state().unwrap().block_height, NEW_BLOCK_HEIGHT);
		});
}

#[test]
fn solana_fees_tracking_works() {
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
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
			System::reset_events();

			assert_eq!(SolanaChainTrackingProvider::priority_fee(), Some(1_000));

			const NEW_FEE: u64 = 2_000u64;

			witness_solana_state(SolanaState::Fee(NEW_FEE));

			testnet.move_forward_blocks(1);

			assert_eq!(SolanaChainTrackingProvider::priority_fee(), Some(NEW_FEE));
		});
}

#[test]
fn solana_delta_based_ingress_works() {
	super::genesis::with_test_defaults()
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.blocks_per_epoch(EPOCH_BLOCKS)
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
			System::reset_events();

			const SOLANA_BLOCKNUMBER: u64 = Solana::WITNESS_PERIOD * 10u64;
			const INITIAL_AMOUNT: u64 = 1_000_000_000_000u64;
			const FOLLOW_UP_AMOUNT: u64 = 500_000_000_000u64;
			const FINAL_AMOUNT: u64 = 1_500_000_000_000u64;

			witness_solana_state(SolanaState::BlockHeight(SOLANA_BLOCKNUMBER));
			testnet.move_forward_blocks(1);

			// Request 2 deposit channels, one for each asset
			assert_ok!(Swapping::request_swap_deposit_address(
				RuntimeOrigin::signed(ZION),
				Asset::Sol,
				Asset::Usdc,
				EncodedAddress::Eth([0x11; 20]),
				Default::default(),
				None,
				Default::default(),
			));
			assert_ok!(Swapping::request_swap_deposit_address(
				RuntimeOrigin::signed(ZION),
				Asset::Sol,
				Asset::SolUsdc,
				EncodedAddress::Sol([0x22; 32]),
				Default::default(),
				None,
				Default::default(),
			));

			// Generated deposit channel address should be correct.
			let (deposit_address_1, _deposit_bump_1) =
				<AddressDerivation as AddressDerivationApi<Solana>>::generate_address_and_state(
					SolAsset::Sol, 1,
				)
				.expect("Must be able to derive Solana deposit channel.");
			let (deposit_address_2, _deposit_bump_2) =
				<AddressDerivation as AddressDerivationApi<Solana>>::generate_address_and_state(
					SolAsset::SolUsdc, 2,
				)
				.expect("Must be able to derive Solana deposit channel.");

			// Witness some Solana ingress for both deposit channels
			testnet.move_forward_blocks(10);
			witness_solana_state(SolanaState::Ingressed(vec![(
				deposit_address_1,
				ChannelTotalIngressedFor::<SolanaIngressEgress> {
					block_number: SOLANA_BLOCKNUMBER + 1,
					amount: INITIAL_AMOUNT,
				},
			),
			(
				deposit_address_2,
				ChannelTotalIngressedFor::<SolanaIngressEgress> {
					block_number: SOLANA_BLOCKNUMBER + 1,
					amount: INITIAL_AMOUNT,
				},
			)]));
			witness_solana_state(SolanaState::BlockHeight(SOLANA_BLOCKNUMBER + 5));
			testnet.move_forward_blocks(1);

			assert_events_match!(
				Runtime,
				RuntimeEvent::SolanaIngressEgress(
					pallet_cf_ingress_egress::Event::DepositFinalised {
						deposit_address,
						asset,
						amount,
						..
					}
				) if deposit_address == deposit_address_1 && asset == SolAsset::Sol && amount == INITIAL_AMOUNT => (),
				RuntimeEvent::SolanaIngressEgress(
					pallet_cf_ingress_egress::Event::DepositFinalised {
						deposit_address,
						asset,
						amount,
						..
					}
				) if deposit_address == deposit_address_2 && asset == SolAsset::Sol && amount == INITIAL_AMOUNT => ()
			);

			// Ingress some more assets into the deposit channel
			witness_solana_state(SolanaState::Ingressed(vec![(
				deposit_address_1,
				ChannelTotalIngressedFor::<SolanaIngressEgress> {
					block_number: SOLANA_BLOCKNUMBER + 6,
					amount: FINAL_AMOUNT,
				},
			),
			(
				deposit_address_2,
				ChannelTotalIngressedFor::<SolanaIngressEgress> {
					block_number: SOLANA_BLOCKNUMBER + 6,
					amount: FINAL_AMOUNT,
				},
			)]));
			witness_solana_state(SolanaState::BlockHeight(SOLANA_BLOCKNUMBER + 10));
			testnet.move_forward_blocks(1);

			assert_events_match!(
				Runtime,
				RuntimeEvent::SolanaIngressEgress(
					pallet_cf_ingress_egress::Event::DepositFinalised {
						deposit_address,
						asset,
						amount,
						..
					}
				) if deposit_address == deposit_address_1 && asset == SolAsset::Sol && amount == FOLLOW_UP_AMOUNT => (),
				RuntimeEvent::SolanaIngressEgress(
					pallet_cf_ingress_egress::Event::DepositFinalised {
						deposit_address,
						asset,
						amount,
						..
					}
				) if deposit_address == deposit_address_2 && asset == SolAsset::Sol && amount == FOLLOW_UP_AMOUNT => ()
			);

			// Simulate assets have been fetched from the account.
			witness_solana_state(SolanaState::Ingressed(vec![(
				deposit_address_1,
				ChannelTotalIngressedFor::<SolanaIngressEgress> {
					block_number: SOLANA_BLOCKNUMBER + 11,
					amount: 0,
				},
			),
			(
				deposit_address_2,
				ChannelTotalIngressedFor::<SolanaIngressEgress> {
					block_number: SOLANA_BLOCKNUMBER + 11,
					amount: 0,
				},
			)]));
			witness_solana_state(SolanaState::BlockHeight(SOLANA_BLOCKNUMBER + 15));
			testnet.move_forward_blocks(1);

			// No ingress should be processed when `total_ingressed_amount` is reduced.
			// The new amount is also ignored and not registered.
			assert!(!System::events().into_iter().any(|event|matches!(event.event, RuntimeEvent::SolanaIngressEgress(
				pallet_cf_ingress_egress::Event::DepositFinalised {..}))));

			testnet.move_forward_blocks(20);

			// Ingress more assets
			witness_solana_state(SolanaState::Ingressed(vec![(
				deposit_address_1,
				ChannelTotalIngressedFor::<SolanaIngressEgress> {
					block_number: SOLANA_BLOCKNUMBER + 16,
					amount: FINAL_AMOUNT,
				},
			),
			(
				deposit_address_2,
				ChannelTotalIngressedFor::<SolanaIngressEgress> {
					block_number: SOLANA_BLOCKNUMBER + 16,
					amount: FINAL_AMOUNT,
				},
			)]));
			witness_solana_state(SolanaState::BlockHeight(SOLANA_BLOCKNUMBER + 20));
			testnet.move_forward_blocks(1);

			// no new ingress is registered.
			assert!(!System::events().into_iter().any(|event|matches!(event.event, RuntimeEvent::SolanaIngressEgress(
				pallet_cf_ingress_egress::Event::DepositFinalised {..}))));
		});
}

#[test]
fn solana_nonce_tracking_works() {
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
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
			System::reset_events();

			const NEW_HASH_1: SolHash = SolHash([0x11; 32]);
			const NEW_HASH_2: SolHash = SolHash([0x22; 32]);
			const NEW_HASH_3: SolHash = SolHash([0x33; 32]);
			// Use up a Solana durable Nonce.
			witness_solana_state(SolanaState::BlockHeight(600));
			let (nonce_account_1, _) =
				SolEnvironment::nonce_account().expect("Must have enough Solana Nonce accounts.");
			let (nonce_account_2, _) =
				SolEnvironment::nonce_account().expect("Must have enough Solana Nonce accounts.");
			let (nonce_account_3, _) =
				SolEnvironment::nonce_account().expect("Must have enough Solana Nonce accounts.");
			assert!(
				pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::contains_key(
					nonce_account_1
				)
			);
			assert!(
				pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::contains_key(
					nonce_account_2
				)
			);
			assert!(
				pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::contains_key(
					nonce_account_3
				)
			);

			testnet.move_forward_blocks(1);

			// Use Election to recover the used Nonce.
			witness_solana_state(SolanaState::Nonce(nonce_account_1, NEW_HASH_1, 606));
			witness_solana_state(SolanaState::Nonce(nonce_account_2, NEW_HASH_2, 606));
			witness_solana_state(SolanaState::Nonce(nonce_account_3, NEW_HASH_3, 606));
			testnet.move_forward_blocks(1);

			assert!(
				!pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::contains_key(
					nonce_account_1
				)
			);
			assert!(
				!pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::contains_key(
					nonce_account_2
				)
			);
			assert!(
				!pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::contains_key(
					nonce_account_3
				)
			);

			assert!(pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::get()
				.contains(&(nonce_account_1, NEW_HASH_1)));
			assert!(pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::get()
				.contains(&(nonce_account_2, NEW_HASH_2)));
			assert!(pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::get()
				.contains(&(nonce_account_3, NEW_HASH_3)));
		});
}
