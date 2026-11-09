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

//! Failed deposits recovered to the brokers' recovery accounts.

use crate::{genesis, network::register_refund_addresses};
use cf_chains::ForeignChainAddress;
use cf_primitives::{AccountId, AccountRole, Asset, FLIPPERINOS_PER_FLIP};
use cf_traits::FailedDepositRecoveryOutcome;
use frame_support::{assert_ok, traits::OnKilledAccount};
use pallet_cf_asset_balances::FreeBalances;
use pallet_cf_environment::FailedDepositRecoveryRequest;
use pallet_cf_ingress_egress::{FetchOrTransfer, ScheduledEgressFetchOrTransfer};
use state_chain_runtime::{
	Environment, EthereumInstance, Runtime, RuntimeEvent, RuntimeOrigin, Swapping, System,
};

const BROKER: AccountId = AccountId::new([0x31; 32]);
const UNLINKED_BROKER: AccountId = AccountId::new([0x32; 32]);
const RECOVERY_ACCOUNT: AccountId = AccountId::new([0x33; 32]);

const ETH_AMOUNT: u128 = 10u128.pow(18);
const ARB_USDC_AMOUNT: u128 = 1_000_000;

fn request(
	broker: AccountId,
	asset: Asset,
	amount: u128,
	reference: &[u8],
) -> FailedDepositRecoveryRequest<AccountId> {
	FailedDepositRecoveryRequest {
		broker,
		asset,
		amount,
		reference: reference.to_vec().try_into().unwrap(),
	}
}

#[test]
fn batch_of_mixed_outcomes() {
	genesis::with_test_defaults()
		.with_additional_accounts(&[
			(BROKER, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(UNLINKED_BROKER, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(RECOVERY_ACCOUNT, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			// Refund addresses on Ethereum, Polkadot, Bitcoin and Solana, but not Arbitrum.
			register_refund_addresses(&RECOVERY_ACCOUNT);
			assert_ok!(Swapping::link_recovery_account(
				RuntimeOrigin::signed(BROKER),
				RECOVERY_ACCOUNT
			));
			System::reset_events();

			assert_ok!(Environment::recover_failed_deposits(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				vec![
					request(BROKER, Asset::Eth, ETH_AMOUNT, b"eth"),
					// Doesn't cover the egress fee.
					request(BROKER, Asset::Btc, 1, b"btc"),
					// No refund address.
					request(BROKER, Asset::ArbUsdc, ARB_USDC_AMOUNT, b"arb"),
					request(UNLINKED_BROKER, Asset::Usdc, 1_000_000, b"usdc"),
					// Polkadot is deprecated.
					request(BROKER, Asset::Dot, 1_000, b"dot"),
				]
				.try_into()
				.unwrap(),
			));

			let outcomes = System::events()
				.into_iter()
				.filter_map(|record| match record.event {
					RuntimeEvent::Environment(
						pallet_cf_environment::Event::FailedDepositRecovered {
							recovery_account,
							outcome,
							reference,
							..
						},
					) => {
						assert_eq!(recovery_account, RECOVERY_ACCOUNT);
						Some((reference.into_inner(), Some(outcome)))
					},
					RuntimeEvent::Environment(
						pallet_cf_environment::Event::FailedDepositRecoveryFailed {
							reference, ..
						},
					) => Some((reference.into_inner(), None)),
					_ => None,
				})
				.collect::<Vec<_>>();

			assert!(
				matches!(
					&outcomes[..],
					[
						(
							_,
							Some(FailedDepositRecoveryOutcome::Egressed {
								destination: ForeignChainAddress::Eth(_),
								..
							})
						),
						(_, Some(FailedDepositRecoveryOutcome::Credited)),
						(_, Some(FailedDepositRecoveryOutcome::Credited)),
						(_, None),
						(_, None),
					]
				),
				"{outcomes:?}"
			);
			assert_eq!(
				outcomes.iter().map(|(reference, _)| reference.as_slice()).collect::<Vec<_>>(),
				vec![&b"eth"[..], b"btc", b"arb", b"usdc", b"dot"]
			);

			assert!(ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::get().iter().any(
				|request| matches!(
					request,
					FetchOrTransfer::Transfer { destination_address, .. }
						if destination_address.is_zero()
				)
			));
			assert_eq!(FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Eth), 0);
			assert_eq!(FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Btc), 1);
			assert_eq!(
				FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::ArbUsdc),
				ARB_USDC_AMOUNT
			);
			assert_eq!(FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Usdc), 0);
		});
}

#[test]
fn link_survives_deregistration_and_is_removed_when_the_account_is_killed() {
	genesis::with_test_defaults()
		.with_additional_accounts(&[
			(BROKER, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
			(RECOVERY_ACCOUNT, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			assert_ok!(Swapping::link_recovery_account(
				RuntimeOrigin::signed(BROKER),
				RECOVERY_ACCOUNT
			));

			assert_ok!(Swapping::deregister_as_broker(RuntimeOrigin::signed(BROKER)));
			assert_eq!(
				pallet_cf_swapping::BrokerRecoveryAccount::<Runtime>::get(&BROKER),
				Some(RECOVERY_ACCOUNT)
			);

			<Runtime as frame_system::Config>::OnKilledAccount::on_killed_account(&BROKER);

			assert_eq!(pallet_cf_swapping::BrokerRecoveryAccount::<Runtime>::get(&BROKER), None);
		});
}

fn failed_btc_witness(
) -> pallet_cf_ingress_egress::VaultDepositWitness<Runtime, state_chain_runtime::BitcoinInstance> {
	use cf_chains::btc::{deposit_address::DepositAddress, Hash, ScriptPubkey, Utxo, UtxoId};
	pallet_cf_ingress_egress::VaultDepositWitness::Failed {
		deposit: pallet_cf_ingress_egress::VaultDepositFacts {
			input_asset: cf_chains::assets::btc::Asset::Btc,
			deposit_amount: 100_000_000,
			deposit_address: Some(ScriptPubkey::Taproot([1; 32])),
			channel_id: Some(9),
			tx_id: Hash::repeat_byte(1),
			deposit_details: Utxo {
				id: UtxoId { tx_id: Hash::repeat_byte(1), vout: 0 },
				amount: 100_000_000,
				deposit_address: DepositAddress::new([1; 32], 9),
			},
		},
		broker_id: Some(BROKER),
	}
}

fn unrefundable_btc_witness(
) -> pallet_cf_ingress_egress::VaultDepositWitness<Runtime, state_chain_runtime::BitcoinInstance> {
	use cf_chains::{address::EncodedAddress, Bitcoin, Chain, ChannelRefundParametersForChain};
	use cf_primitives::Beneficiary;
	use pallet_cf_ingress_egress::{VaultDepositWitness, VaultSwapData};
	VaultDepositWitness::Swap {
		deposit: failed_btc_witness().deposit().clone(),
		swap: VaultSwapData {
			output_asset: Asset::Eth,
			destination_address: EncodedAddress::Eth([2; 20]),
			deposit_metadata: None,
			broker_fee: Some(Beneficiary { account: BROKER, bps: 0 }),
			affiliate_fees: Default::default(),
			refund_params: ChannelRefundParametersForChain::<Bitcoin> {
				retry_duration: 0,
				refund_address: Bitcoin::BURN_ADDRESS,
				min_price: Default::default(),
				refund_ccm_metadata: None,
				max_oracle_price_slippage: None,
			},
			dca_params: None,
			boost_fee: 0,
		},
	}
}

#[test]
fn failed_and_unrefundable_btc_deposits_credit_the_lp_and_register_the_utxo() {
	use pallet_cf_ingress_egress::{DepositAction, Event, FailedVaultDeposits};
	use state_chain_runtime::{BitcoinIngressEgress, BitcoinInstance};
	for witness in [failed_btc_witness(), unrefundable_btc_witness()] {
		genesis::with_test_defaults()
			.with_additional_accounts(&[
				(BROKER, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
				(RECOVERY_ACCOUNT, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			])
			.build()
			.execute_with(|| {
				assert_ok!(Swapping::link_recovery_account(
					RuntimeOrigin::signed(BROKER),
					RECOVERY_ACCOUNT
				));
				let utxo = witness.deposit().deposit_details.clone();
				let mut expected_utxos =
					pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get();
				expected_utxos.push(utxo);
				BitcoinIngressEgress::process_vault_swap_request_full_witness_inner(100, witness);
				let amount_and_fee = System::events()
					.iter()
					.filter_map(|event| match &event.event {
						RuntimeEvent::BitcoinIngressEgress(Event::DepositFinalised {
							amount,
							ingress_fee,
							action:
								DepositAction::BrokerRecovery {
									recovery_account,
									outcome: FailedDepositRecoveryOutcome::Credited,
									..
								},
							..
						}) => {
							assert_eq!(recovery_account, &RECOVERY_ACCOUNT);
							Some((*amount, *ingress_fee))
						},
						_ => None,
					})
					.collect::<Vec<_>>();
				assert_eq!(amount_and_fee.len(), 1);
				let (amount, fee) = amount_and_fee[0];
				assert_eq!(
					FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Btc),
					u128::from(amount - fee)
				);
				assert!(FailedVaultDeposits::<Runtime, BitcoinInstance>::iter_keys()
					.next()
					.is_none());
				assert_eq!(
					pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get(),
					expected_utxos
				);
			});
	}
}

#[test]
fn rejected_btc_deposits_are_queued_and_register_the_utxo() {
	use pallet_cf_ingress_egress::{FailedVaultDepositCause, FailedVaultDeposits};
	use state_chain_runtime::{BitcoinIngressEgress, BitcoinInstance};
	for witness in [failed_btc_witness(), unrefundable_btc_witness()] {
		genesis::with_test_defaults()
			.with_additional_accounts(&[
				(BROKER, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
				(RECOVERY_ACCOUNT, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			])
			.build()
			.execute_with(|| {
				assert_ok!(Swapping::link_recovery_account(
					RuntimeOrigin::signed(BROKER),
					RECOVERY_ACCOUNT
				));
				assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
					RuntimeOrigin::signed(BROKER),
					witness.deposit().tx_id
				));
				let mut expected_utxos =
					pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get();
				expected_utxos.push(witness.deposit().deposit_details.clone());

				BitcoinIngressEgress::process_vault_swap_request_full_witness_inner(100, witness);

				let entry =
					FailedVaultDeposits::<Runtime, BitcoinInstance>::iter_values().next().unwrap();
				assert_eq!(entry.cause, FailedVaultDepositCause::TransactionRejected);
				assert_eq!(entry.deposit.broker_id, Some(BROKER));
				assert_eq!(FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Btc), 0);
				assert_eq!(
					pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get(),
					expected_utxos
				);
			});
	}
}

#[test]
fn missing_or_inactive_link_queues_btc_until_governance_processes_it() {
	use cf_traits::{AccountRoleRegistry, BrokerRecoveryApi};
	use pallet_cf_ingress_egress::FailedVaultDeposits;
	use state_chain_runtime::{AccountRoles, BitcoinIngressEgress, BitcoinInstance};
	for inactive in [false, true] {
		for witness in [failed_btc_witness(), unrefundable_btc_witness()] {
			genesis::with_test_defaults()
				.with_additional_accounts(&[
					(BROKER, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
					(RECOVERY_ACCOUNT, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
				])
				.build()
				.execute_with(|| {
					if inactive {
						assert_ok!(Swapping::link_recovery_account(
							RuntimeOrigin::signed(BROKER),
							RECOVERY_ACCOUNT
						));
						assert_ok!(AccountRoles::deregister_account_role(
							&RECOVERY_ACCOUNT,
							AccountRole::LiquidityProvider
						));
					}
					assert_eq!(Swapping::recovery_account(&BROKER), None);
					let queue_id = BitcoinIngressEgress::failed_vault_deposit_id(&witness);
					let initial_utxos =
						pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get().len();
					BitcoinIngressEgress::process_vault_swap_request_full_witness_inner(
						100,
						witness.clone(),
					);
					let entry =
						FailedVaultDeposits::<Runtime, BitcoinInstance>::get(queue_id).unwrap();
					assert_eq!(entry.deposit.broker_id, Some(BROKER));
					assert_eq!(
						entry.deposit.remaining_amount + entry.deposit.ingress_fee,
						witness.deposit().deposit_amount
					);
					let utxos = pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get();
					assert_eq!(utxos.len(), initial_utxos + 1);
					assert!(utxos.contains(&witness.deposit().deposit_details));
					let gov = || pallet_cf_governance::RawOrigin::GovernanceApproval.into();
					frame_support::assert_noop!(
						BitcoinIngressEgress::attribute_failed_vault_deposit(gov(), queue_id, BROKER),
						pallet_cf_ingress_egress::Error::<Runtime, BitcoinInstance>::NoRecoveryAccount
					);
					assert_eq!(
						FailedVaultDeposits::<Runtime, BitcoinInstance>::get(queue_id),
						Some(entry.clone())
					);
					if inactive {
						assert_ok!(AccountRoles::register_as_liquidity_provider(&RECOVERY_ACCOUNT));
					} else {
						assert_ok!(Swapping::link_recovery_account(
							RuntimeOrigin::signed(BROKER),
							RECOVERY_ACCOUNT
						));
					}
					assert_eq!(FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Btc), 0);
					assert_ok!(BitcoinIngressEgress::attribute_failed_vault_deposit(
						gov(),
						queue_id,
						BROKER
					));
					assert_eq!(
						FreeBalances::<Runtime>::get(&RECOVERY_ACCOUNT, Asset::Btc),
						u128::from(entry.deposit.remaining_amount)
					);
					assert!(FailedVaultDeposits::<Runtime, BitcoinInstance>::iter_keys()
						.next()
						.is_none());
					frame_support::assert_noop!(
						BitcoinIngressEgress::attribute_failed_vault_deposit(gov(), queue_id, BROKER),
						pallet_cf_ingress_egress::Error::<Runtime, BitcoinInstance>::UnknownFailedVaultDeposit
					);
				});
		}
	}
}

#[test]
fn unattributed_evm_and_solana_deposits_queue_including_multiple_deposits_per_tx() {
	use cf_chains::{
		evm::{DepositDetails, H256},
		sol::{SolAddress, VaultSwapOrDepositChannelId},
	};
	use pallet_cf_ingress_egress::{FailedVaultDeposits, VaultDepositFacts, VaultDepositWitness};
	use state_chain_runtime::{EthereumIngressEgress, SolanaIngressEgress, SolanaInstance};
	genesis::with_test_defaults().build().execute_with(|| {
		let tx_id = H256::repeat_byte(5);
		for amount in [ETH_AMOUNT, ETH_AMOUNT * 2] {
			let witness = VaultDepositWitness::Failed {
				deposit: VaultDepositFacts {
					input_asset: cf_chains::assets::eth::Asset::Eth,
					deposit_amount: amount,
					deposit_address: None,
					channel_id: None,
					tx_id,
					deposit_details: DepositDetails { tx_hashes: Some(vec![tx_id]) },
				},
				broker_id: None,
			};
			let queue_id = EthereumIngressEgress::failed_vault_deposit_id(&witness);
			EthereumIngressEgress::process_vault_swap_request_full_witness_inner(100, witness);
			assert!(FailedVaultDeposits::<Runtime, EthereumInstance>::contains_key(queue_id));
		}
		assert_eq!(FailedVaultDeposits::<Runtime, EthereumInstance>::iter_keys().count(), 2);
		let sol_id = (SolAddress::from([7; 32]), 42);
		let witness = VaultDepositWitness::Failed {
			deposit: VaultDepositFacts {
				input_asset: cf_chains::assets::sol::Asset::Sol,
				deposit_amount: 1_000_000_000,
				deposit_address: Some(sol_id.0),
				channel_id: None,
				tx_id: sol_id,
				deposit_details: VaultSwapOrDepositChannelId::VaultSwapAccount(sol_id),
			},
			broker_id: None,
		};
		let queue_id = SolanaIngressEgress::failed_vault_deposit_id(&witness);
		SolanaIngressEgress::process_vault_swap_request_full_witness_inner(100, witness);
		assert!(FailedVaultDeposits::<Runtime, SolanaInstance>::contains_key(queue_id));
	});
}
