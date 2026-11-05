// Copyright 2026 Chainflip Labs GmbH
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{FailedVaultDepositCause, FailedVaultDeposits};
use cf_test_utilities::assert_no_matching_event;
use cf_traits::{
	mocks::failed_deposit_recovery::MockBrokerRecoveryApi, FailedDepositRecoveryOutcome,
};
use frame_support::assert_noop;

const RECOVERY_LP: u64 = 11;

fn failed_eth(amount: u128, broker_id: Option<u64>) -> VaultDepositWitness<Test, Instance1> {
	let mut deposit = eth_vault_deposit_witness().deposit().clone();
	deposit.deposit_amount = amount;
	deposit.deposit_details = DepositDetails { tx_hashes: Some(vec![deposit.tx_id]) };
	VaultDepositWitness::Failed { deposit, broker_id }
}

fn unrefundable_eth(amount: u128, broker_id: Option<u64>) -> VaultDepositWitness<Test, Instance1> {
	let mut witness = eth_vault_deposit_witness();
	witness.deposit_mut().deposit_amount = amount;
	let swap = witness.swap_mut().unwrap();
	swap.refund_params.refund_address = Ethereum::BURN_ADDRESS;
	swap.broker_fee = broker_id.map(|account| Beneficiary { account, bps: 0 });
	witness
}

fn submit(witness: VaultDepositWitness<Test, Instance1>) {
	assert_ok!(EthereumIngressEgress::vault_swap_request(
		RuntimeOrigin::root(),
		42,
		Box::new(witness)
	));
}

fn queue_count() -> usize {
	FailedVaultDeposits::<Test, Instance1>::iter_keys().count()
}

fn finalised_count() -> usize {
	System::events()
		.iter()
		.filter(|event| {
			matches!(
				event.event,
				RuntimeEvent::EthereumIngressEgress(Event::DepositFinalised { .. })
			)
		})
		.count()
}

#[test]
fn failed_deposits_recover_to_the_lp_without_refunding_the_user() {
	new_test_ext().execute_with(|| {
		ChainTracker::<Ethereum>::set_fee(100);
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
		submit(failed_eth(1_000, Some(BROKER)));
		assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 900);
		assert_eq!(MockAssetWithholding::withheld_assets(Asset::Eth), 100);
		assert!(FailedVaultDeposits::<Test, Instance1>::iter().next().is_none());
		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());
		assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
		assert_has_matching_event!(
			Test,
			RuntimeEvent::EthereumIngressEgress(Event::DepositFinalised {
				ingress_fee: 100,
				action: DepositAction::BrokerRecovery {
					broker_id: BROKER,
					recovery_account: RECOVERY_LP,
					outcome: FailedDepositRecoveryOutcome::Credited
				},
				..
			})
		);
		assert_eq!(finalised_count(), 1);
	});
}

#[test]
fn failed_deposits_can_egress_to_the_lp_refund_address() {
	use cf_traits::{mocks::balance_api::MockRefundAddressRegistry, RefundAddressRegistry};
	new_test_ext().execute_with(|| {
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
		MockRefundAddressRegistry::register_liquidity_refund_address(
			&RECOVERY_LP,
			ForeignChainAddress::Eth(H160([7; 20])),
		);
		ChainTracker::<Ethereum>::set_fee(100);
		submit(failed_eth(1_000, Some(BROKER)));
		assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 0);
		assert_has_matching_event!(
			Test,
			RuntimeEvent::EthereumIngressEgress(Event::FailedDepositRecovered {
				queue_id: None,
				outcome: FailedDepositRecoveryOutcome::Egressed {
					egress_amount: 800,
					fee_withheld: 100,
					..
				},
				..
			})
		);
		assert_eq!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().len(), 1);
	});
}

#[test]
fn unavailable_attribution_or_recovery_queues_exact_remaining_liability() {
	for broker in [None, Some(BROKER)] {
		for witness in [failed_eth(1_000, broker), unrefundable_eth(1_000, broker)] {
			new_test_ext().execute_with(|| {
				ChainTracker::<Ethereum>::set_fee(100);
				submit(witness.clone());
				let entry = FailedVaultDeposits::<Test, Instance1>::iter_values().next().unwrap();
				assert_eq!(entry.deposit.facts, *witness.deposit());
				assert_eq!(entry.deposit.block_height, 42);
				assert_eq!(entry.deposit.recorded_at, System::block_number());
				assert_eq!(entry.deposit.broker_id, broker);
				assert_eq!(entry.deposit.ingress_fee, 100);
				assert_eq!(entry.deposit.remaining_amount, 900);
				assert_eq!(
					entry.cause,
					if broker.is_some() {
						FailedVaultDepositCause::RecoveryFailed(
							crate::Error::<Test, Instance1>::NoRecoveryAccount.into(),
						)
					} else if witness.swap().is_some() {
						FailedVaultDepositCause::Unrefundable
					} else {
						FailedVaultDepositCause::UnknownBroker
					}
				);
				assert_eq!(queue_count(), 1);
				assert_eq!(finalised_count(), 1);
				assert!(!System::events().iter().any(|event| matches!(
					event.event,
					RuntimeEvent::EthereumIngressEgress(Event::DepositFailed { .. })
				)));
			});
		}
	}
}

#[test]
fn decoded_unrefundable_swap_recovers_to_the_lp() {
	use cf_traits::{mocks::balance_api::MockRefundAddressRegistry, RefundAddressRegistry};
	for egress in [false, true] {
		new_test_ext().execute_with(|| {
			ChainTracker::<Ethereum>::set_fee(100);
			MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
			if egress {
				MockRefundAddressRegistry::register_liquidity_refund_address(
					&RECOVERY_LP,
					ForeignChainAddress::Eth(H160([7; 20])),
				);
			}
			submit(unrefundable_eth(1_000, Some(BROKER)));
			assert_eq!(queue_count(), 0);
			assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
			assert_has_matching_event!(
				Test,
				RuntimeEvent::EthereumIngressEgress(Event::FailedDepositRecovered {
					queue_id: None,
					broker_id: BROKER,
					recovery_account: RECOVERY_LP,
					deposit: crate::FailedVaultDepositInfo {
						ingress_fee: 100,
						remaining_amount: 900,
						..
					},
					..
				})
			);
			if egress {
				assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 0);
				assert_eq!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().len(), 1);
				assert_has_matching_event!(
					Test,
					RuntimeEvent::EthereumIngressEgress(Event::FailedDepositRecovered {
						outcome: FailedDepositRecoveryOutcome::Egressed {
							destination: ForeignChainAddress::Eth(destination),
							egress_amount: 800,
							fee_withheld: 100,
							..
						},
						..
					}) if *destination == H160([7; 20])
				);
				assert_eq!(MockAssetWithholding::withheld_assets(Asset::Eth), 200);
			} else {
				assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 900);
				assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());
				assert_eq!(MockAssetWithholding::withheld_assets(Asset::Eth), 100);
			}
			assert_has_matching_event!(
				Test,
				RuntimeEvent::EthereumIngressEgress(Event::DepositFinalised {
					action: DepositAction::BrokerRecovery { .. },
					..
				})
			);
			assert_eq!(finalised_count(), 1);
		});
	}
}

#[test]
fn governance_reassigns_and_pays_once_without_repeating_ingress_fees() {
	new_test_ext().execute_with(|| {
		ChainTracker::<Ethereum>::set_fee(100);
		let witness = failed_eth(1_000, Some(BROKER));
		let id = EthereumIngressEgress::failed_vault_deposit_id(&witness);
		submit(witness.clone());
		let entry = FailedVaultDeposits::<Test, Instance1>::iter_values().next().unwrap();
		assert_noop!(EthereumIngressEgress::attribute_failed_vault_deposit(RuntimeOrigin::signed(BROKER), id, BROKER), DispatchError::BadOrigin);
		assert_noop!(EthereumIngressEgress::attribute_failed_vault_deposit(RuntimeOrigin::root(), id, BROKER), crate::Error::<Test, Instance1>::NoRecoveryAccount);
		assert_eq!(FailedVaultDeposits::<Test, Instance1>::get(id), Some(entry));
		let new_broker = BROKER + 1;
		MockBrokerRecoveryApi::link(new_broker, RECOVERY_LP);
		assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 0);
		assert_ok!(EthereumIngressEgress::attribute_failed_vault_deposit(RuntimeOrigin::root(), id, new_broker));
		assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 900);
		assert_eq!(MockAssetWithholding::withheld_assets(Asset::Eth), 100);
		assert_eq!(queue_count(), 0);
		assert_noop!(EthereumIngressEgress::attribute_failed_vault_deposit(RuntimeOrigin::root(), id, new_broker), crate::Error::<Test, Instance1>::UnknownFailedVaultDeposit);
		assert_eq!(finalised_count(), 1);
		assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 900);
		assert_has_matching_event!(Test, RuntimeEvent::EthereumIngressEgress(Event::FailedDepositRecovered {
			queue_id: Some(queue_id), broker_id, deposit: crate::FailedVaultDepositInfo { broker_id: Some(BROKER), .. }, ..
		}) if *broker_id == new_broker && *queue_id == id);
	});
}

#[test]
fn distinct_evm_deposits_in_one_transaction_can_be_recovered_independently() {
	new_test_ext().execute_with(|| {
		let first = failed_eth(1_000, None);
		let second = failed_eth(2_000, None);
		assert_eq!(first.deposit().tx_id, second.deposit().tx_id);
		let first_id = EthereumIngressEgress::failed_vault_deposit_id(&first);
		let second_id = EthereumIngressEgress::failed_vault_deposit_id(&second);
		submit(first);
		submit(second);
		let mut decoded = eth_vault_deposit_witness();
		decoded.swap_mut().unwrap().refund_params.refund_address = Ethereum::BURN_ADDRESS;
		submit(decoded.clone());
		decoded.swap_mut().unwrap().boost_fee = 5;
		submit(decoded);
		assert_eq!(queue_count(), 4);
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
		assert_ok!(EthereumIngressEgress::attribute_failed_vault_deposit(
			RuntimeOrigin::root(),
			first_id,
			BROKER
		));
		assert!(!FailedVaultDeposits::<Test, Instance1>::contains_key(first_id));
		assert_eq!(
			FailedVaultDeposits::<Test, Instance1>::get(second_id)
				.unwrap()
				.deposit
				.remaining_amount,
			2_000
		);
		assert_eq!(queue_count(), 3);
		assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 1_000);
		assert_eq!(finalised_count(), 4);
	});
}

#[test]
fn failed_and_unrefundable_witnesses_never_enter_boosting() {
	let mut unrefundable = eth_vault_deposit_witness();
	unrefundable.swap_mut().unwrap().refund_params.refund_address = Ethereum::BURN_ADDRESS;
	unrefundable.swap_mut().unwrap().boost_fee = 5;
	for witness in [failed_eth(1_000, Some(BROKER)), unrefundable] {
		new_test_ext().execute_with(|| {
			MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
			MockBoostApi::set_available_amount(1_000);
			let tx_id = witness.deposit().tx_id;
			EthereumIngressEgress::process_vault_swap_request_prewitness(42, witness);
			assert_eq!(
				crate::BoostedVaultTransactions::<Test, Instance1>::get(tx_id),
				BoostStatus::NotBoosted
			);
			assert_eq!(MockBoostApi::get_available_amount(), 1_000);
			assert_eq!(queue_count(), 0);
			assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 0);
			assert_eq!(finalised_count(), 0);
		});
	}
}

#[test]
fn minimum_and_fee_exhaustion_do_not_create_liabilities() {
	for amount in [99, 100] {
		new_test_ext().execute_with(|| {
			crate::MinimumDeposit::<Test, Instance1>::insert(EthAsset::Eth, 100);
			ChainTracker::<Ethereum>::set_fee(100);
			submit(failed_eth(amount, None));
			assert_eq!(queue_count(), 0);
			assert_eq!(finalised_count(), 0);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::EthereumIngressEgress(Event::DepositFailed { .. })
			);
		});
	}
}

#[test]
fn screening_does_not_consume_reports_or_prevent_queueing() {
	new_test_ext().execute_with(|| {
		let witness = failed_eth(1_000, Some(BROKER));
		let status = crate::TransactionRejectionStatus { prewitnessed: false, expires_at: 100 };
		crate::TransactionsMarkedForRejection::<Test, Instance1>::insert(
			BROKER,
			witness.deposit().tx_id,
			&status,
		);
		submit(witness.clone());
		assert_eq!(queue_count(), 1);
		assert_eq!(
			crate::TransactionsMarkedForRejection::<Test, Instance1>::get(
				BROKER,
				witness.deposit().tx_id
			),
			Some(status)
		);
		assert!(crate::ScheduledTransactionsForRejection::<Test, Instance1>::get().is_empty());
		assert!(crate::ReportExpiresAt::<Test, Instance1>::iter().next().is_none());
	});
}

proptest::proptest! {
	#[test]
	fn recovery_conserves_the_post_fee_liability(
		net_amount in 1u128..1_000_000_000,
		fee in 0u128..100_000,
		linked in proptest::bool::ANY,
		decoded in proptest::bool::ANY,
	) {
		new_test_ext().execute_with(|| {
			ChainTracker::<Ethereum>::set_fee(fee);
			if linked { MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP); }
			let witness = if decoded {
				unrefundable_eth(net_amount + fee, Some(BROKER))
			} else {
				failed_eth(net_amount + fee, Some(BROKER))
			};
			let id = EthereumIngressEgress::failed_vault_deposit_id(&witness);
			submit(witness);
			let queued = FailedVaultDeposits::<Test, Instance1>::iter_values()
				.map(|entry| entry.deposit.remaining_amount).sum::<u128>();
			assert_eq!(queued + MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), net_amount);
			assert_eq!(MockAssetWithholding::withheld_assets(Asset::Eth), fee);
			if !linked {
				MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
				assert_ok!(EthereumIngressEgress::attribute_failed_vault_deposit(RuntimeOrigin::root(), id, BROKER));
			}
			assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), net_amount);
			assert_eq!(queue_count(), 0);
			assert_eq!(finalised_count(), 1);
		});
	}
}

#[test]
fn recovery_credit_rolls_back_failed_egress_fees() {
	use cf_traits::{mocks::balance_api::MockRefundAddressRegistry, RefundAddressRegistry};
	new_test_ext().execute_with(|| {
		ChainTracker::<Ethereum>::set_fee(100);
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
		MockRefundAddressRegistry::register_liquidity_refund_address(
			&RECOVERY_LP,
			ForeignChainAddress::Eth(H160([7; 20])),
		);
		submit(failed_eth(150, Some(BROKER)));
		assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 50);
		assert_eq!(MockAssetWithholding::withheld_assets(Asset::Eth), 100);
		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance1>::get().is_empty());
	});
}

#[test]
fn uneconomic_user_refund_preserves_its_existing_policy() {
	for ccm in [false, true] {
		new_test_ext().execute_with(|| {
			ChainTracker::<Ethereum>::set_fee(100);
			crate::EgressDustLimit::<Test, Instance1>::insert(EthAsset::Eth, 1);
			MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
			let mut witness = eth_vault_deposit_witness();
			witness.deposit_mut().deposit_amount = 150;
			let swap = witness.swap_mut().unwrap();
			// Refund instructions pass validation, while the output address selects a refund.
			swap.destination_address = EncodedAddress::Btc(vec![]);
			if ccm {
				swap.refund_params.refund_ccm_metadata = Some(CcmChannelMetadataUnchecked {
					message: bounded_vec![1],
					gas_budget: 1,
					ccm_additional_data: Default::default(),
				});
			}
			submit(witness);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::EthereumIngressEgress(Event::DepositFinalised {
					action: DepositAction::Refund { egress_id: None, .. },
					..
				})
			);
			assert_eq!(queue_count(), 0);
			assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 0);
			assert_eq!(MockAssetWithholding::withheld_assets(Asset::Eth), 150);
		});
	}
}

#[test]
fn boosted_swap_still_settles_if_broker_validation_changes() {
	use cf_traits::mocks::account_role_registry::MockAccountRoleRegistry;
	new_test_ext().execute_with(|| {
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
		MockBoostApi::set_available_amount(1_000);
		let mut witness = eth_vault_deposit_witness();
		witness.deposit_mut().deposit_amount = 1_000;
		witness.swap_mut().unwrap().boost_fee = 5;
		let tx_id = witness.deposit().tx_id;
		EthereumIngressEgress::process_vault_swap_request_prewitness(42, witness.clone());
		assert!(matches!(
			crate::BoostedVaultTransactions::<Test, Instance1>::get(tx_id),
			BoostStatus::Boosted { amount: 1_000, .. }
		));
		assert_ok!(
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::deregister_account_role(
				&BROKER,
				cf_primitives::AccountRole::Broker
			)
		);
		submit(witness);
		assert_eq!(MockBoostApi::get_available_amount(), 1_000);
		assert_eq!(
			crate::BoostedVaultTransactions::<Test, Instance1>::get(tx_id),
			BoostStatus::NotBoosted
		);
		assert_eq!(queue_count(), 0);
		assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 0);
		assert_has_matching_event!(
			Test,
			RuntimeEvent::EthereumIngressEgress(Event::UnknownBroker { broker_id: BROKER })
		);
		assert_has_matching_event!(
			Test,
			RuntimeEvent::EthereumIngressEgress(Event::DepositFinalised {
				action: DepositAction::BoostersCredited { .. },
				..
			})
		);
	});
}

#[test]
fn decoded_unrefundable_deposit_leaves_screening_for_a_valid_deposit_in_the_same_tx() {
	new_test_ext().execute_with(|| {
		let mut witness = eth_vault_deposit_witness();
		witness.deposit_mut().deposit_details.tx_hashes = Some(vec![EvmHash::default()]);
		let status = crate::TransactionRejectionStatus { prewitnessed: false, expires_at: 100 };
		crate::TransactionsMarkedForRejection::<Test, Instance1>::insert(
			BROKER,
			EvmHash::default(),
			&status,
		);
		let mut unrefundable = witness.clone();
		unrefundable.swap_mut().unwrap().refund_params.refund_address = Ethereum::BURN_ADDRESS;
		submit(unrefundable);
		assert_eq!(queue_count(), 1);
		assert_eq!(
			crate::TransactionsMarkedForRejection::<Test, Instance1>::get(
				BROKER,
				EvmHash::default()
			),
			Some(status)
		);
		submit(witness);
		assert_eq!(crate::ScheduledTransactionsForRejection::<Test, Instance1>::get().len(), 1);
		assert_eq!(queue_count(), 1);
		assert_eq!(finalised_count(), 1);
	});
}

/// Only deposits without a usable refund address go to the broker; a marked deposit that can be
/// refunded is rejected back to the depositor, whoever marked it.
#[test]
fn marked_refundable_deposits_are_rejected_even_with_a_recovery_account() {
	let mut refund_action = eth_vault_deposit_witness();
	refund_action.swap_mut().unwrap().destination_address = EncodedAddress::Btc(vec![]);
	for marked_by in [BROKER, SCREENING_ID] {
		for witness in [eth_vault_deposit_witness(), refund_action.clone()] {
			new_test_ext().execute_with(|| {
				MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
				let mut witness = witness.clone();
				let tx_id = witness.deposit().tx_id;
				witness.deposit_mut().deposit_amount = 1_000;
				witness.deposit_mut().deposit_details.tx_hashes = Some(vec![tx_id]);
				crate::TransactionsMarkedForRejection::<Test, Instance1>::insert(
					marked_by,
					tx_id,
					crate::TransactionRejectionStatus { prewitnessed: false, expires_at: 100 },
				);

				submit(witness);

				let rejections = crate::ScheduledTransactionsForRejection::<Test, Instance1>::get();
				assert_eq!(rejections.len(), 1);
				assert_eq!(
					rejections[0].refund_address,
					ForeignChainAddress::Eth(ETH_REFUND_PARAMS.refund_address)
				);
				assert_eq!(rejections[0].amount, 1_000);
				assert_has_matching_event!(
					Test,
					RuntimeEvent::EthereumIngressEgress(Event::DepositFailed {
						reason: DepositFailedReason::TransactionRejectedByBroker,
						details: DepositFailedDetails::Vault { .. },
						..
					})
				);
				assert_eq!(queue_count(), 0);
				assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Eth), 0);
				assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
				assert_eq!(finalised_count(), 0);
				assert_no_matching_event!(
					Test,
					RuntimeEvent::EthereumIngressEgress(Event::FailedDepositRecovered { .. })
				);
			});
		}
	}
}

#[test]
fn rejected_refund_ccm_recovers_a_decoded_swap_to_the_lp() {
	new_test_ext().execute_with(|| {
		MockBrokerRecoveryApi::link(BROKER, RECOVERY_LP);
		let mut witness = btc_vault_deposit_witness();
		witness.swap_mut().unwrap().refund_params.refund_ccm_metadata =
			Some(CcmChannelMetadataUnchecked {
				message: bounded_vec![1],
				gas_budget: 1,
				ccm_additional_data: Default::default(),
			});
		let amount = witness.deposit().deposit_amount;
		BitcoinIngressEgress::process_vault_swap_request_full_witness_inner(42, witness);
		assert!(FailedVaultDeposits::<Test, Instance2>::iter_keys().next().is_none());
		assert_eq!(MockBalance::get_balance(&RECOVERY_LP, Asset::Btc), amount.into());
		assert_has_matching_event!(
			Test,
			RuntimeEvent::BitcoinIngressEgress(Event::FailedDepositRecovered {
				queue_id: None,
				broker_id: BROKER,
				recovery_account: RECOVERY_LP,
				outcome: FailedDepositRecoveryOutcome::Credited,
				..
			})
		);
		assert!(ScheduledEgressFetchOrTransfer::<Test, Instance2>::get().is_empty());
	});
}
