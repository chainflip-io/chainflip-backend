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

use super::*;
use crate::{
	tests::{ALICE, BROKER},
	Asset, DepositChannelLookup, DepositFailedDetails, DepositFailedReason, DepositWitness, Event,
	ReportExpiresAt, ScheduledEgressFetchOrTransfer, ScheduledTransactionsForRejection,
	TransactionRejectionStatus, TransactionsMarkedForRejection, VaultDepositWitness,
	MARKED_TX_EXPIRATION_BLOCKS,
};
use cf_chains::{
	address::EncodedAddress,
	btc::{deposit_address::DepositAddress, Hash, ScriptPubkey, UtxoId},
	Bitcoin, ForeignChainAddress,
};
use cf_primitives::{chains::assets::btc, Beneficiaries, Beneficiary, ChannelId};
use cf_test_utilities::{assert_has_event, assert_has_matching_event};
use cf_traits::{mocks::swap_request_api::MockSwapRequestHandler, DepositApi};
use frame_support::{
	assert_noop, assert_ok,
	instances::Instance2,
	traits::{Hooks, OriginTrait},
	weights::Weight,
};
use sp_core::U256;
use sp_runtime::DispatchError::BadOrigin;

const DEFAULT_DEPOSIT_AMOUNT: u64 = 1_000;
const DEFAULT_BTC_ADDRESS: [u8; 20] = [0; 20];

mod helpers {
	use super::*;
	use cf_chains::{btc::Utxo, Bitcoin};

	pub fn generate_btc_deposit(tx_id: Hash) -> Utxo {
		Utxo {
			amount: DEFAULT_DEPOSIT_AMOUNT,
			id: UtxoId { tx_id, vout: 0 },
			deposit_address: DepositAddress { pubkey_x: [0; 32], script_path: None },
		}
	}

	pub fn request_address_and_deposit(
		who: ChannelId,
		asset: btc::Asset,
		deposit_details: Utxo,
	) -> (ChannelId, <Bitcoin as Chain>::ChainAccount) {
		let (id, address, ..) = BitcoinIngressEgress::request_liquidity_deposit_address(
			who,
			asset,
			0,
			ForeignChainAddress::Btc(ScriptPubkey::P2SH(DEFAULT_BTC_ADDRESS)),
		)
		.unwrap();
		let address: <Bitcoin as Chain>::ChainAccount = address.try_into().unwrap();
		assert_ok!(BitcoinIngressEgress::process_channel_deposit_full_witness_inner(
			&DepositWitness {
				deposit_address: address.clone(),
				asset,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details
			},
			Default::default()
		));
		(id, address)
	}

	pub fn setup_boost_swap() -> ForeignChainAddress {
		let (_, address, _, _) = BitcoinIngressEgress::request_swap_deposit_address(
			btc::Asset::Btc,
			btc::Asset::Btc.into(),
			ForeignChainAddress::Btc(ScriptPubkey::P2SH(DEFAULT_BTC_ADDRESS)),
			Beneficiaries::new(),
			BROKER,
			None,
			10,
			ChannelRefundParametersForChain::<Bitcoin> {
				retry_duration: 100,
				refund_address: ScriptPubkey::Taproot([0x01; 32]),
				min_price: U256::from(0),
				refund_ccm_metadata: None,
				max_oracle_price_slippage: None,
			},
			None,
		)
		.unwrap();

		MockBoostApi::set_available_amount(DEFAULT_DEPOSIT_AMOUNT.into());

		address
	}
}

#[test]
fn process_marked_transaction_and_expect_refund() {
	new_test_ext().execute_with(|| {
		let tx_in_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_in_id);
		let (_, address) =
			helpers::request_address_and_deposit(BROKER, btc::Asset::Btc, deposit_details.clone());
		let _ = DepositChannelLookup::<Test, Instance2>::get(address.clone()).unwrap();

		assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			tx_in_id,
		));

		assert_ok!(BitcoinIngressEgress::process_channel_deposit_full_witness_inner(
			&DepositWitness {
				deposit_address: address.clone(),
				asset: btc::Asset::Btc,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details
			},
			Default::default()
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::BitcoinIngressEgress(Event::DepositFailed {
				details: DepositFailedDetails::DepositChannel {
					deposit_witness: DepositWitness {
						deposit_address: _,
						asset: btc::Asset::Btc,
						amount: DEFAULT_DEPOSIT_AMOUNT,
						deposit_details: _,
					},
				},
				reason: DepositFailedReason::TransactionRejectedByBroker,
				block_height: _,
			})
		);

		assert_eq!(ScheduledTransactionsForRejection::<Test, Instance2>::decode_len(), Some(1));
		assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
	});
}

#[test]
fn finalize_boosted_tx_if_marked_after_prewitness() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_id);

		let address: <Bitcoin as Chain>::ChainAccount =
			helpers::setup_boost_swap().try_into().unwrap();

		let _ = BitcoinIngressEgress::process_channel_deposit_prewitness(
			DepositWitness {
				deposit_address: address.clone(),
				asset: btc::Asset::Btc,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: deposit_details.clone(),
			},
			10,
		);

		// It's possible to report the tx, but reporting will have no effect.
		assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			tx_id,
		),);

		assert_ok!(BitcoinIngressEgress::process_channel_deposit_full_witness_inner(
			&DepositWitness {
				deposit_address: address.clone(),
				asset: btc::Asset::Btc,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details
			},
			Default::default()
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::BitcoinIngressEgress(Event::DepositFinalised {
				deposit_address: _,
				asset: btc::Asset::Btc,
				..
			})
		);

		assert!(MockSwapRequestHandler::<Test>::get_swap_requests().len() == 1);
	});
}

#[test]
fn reject_tx_if_marked_before_prewitness() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_id);

		let address: <Bitcoin as Chain>::ChainAccount =
			helpers::setup_boost_swap().try_into().unwrap();

		assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			tx_id,
		));

		assert_ok!(BitcoinIngressEgress::process_channel_deposit_prewitness(
			DepositWitness {
				deposit_address: address.clone(),
				asset: btc::Asset::Btc,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: deposit_details.clone(),
			},
			10,
		));

		assert_ok!(BitcoinIngressEgress::process_channel_deposit_full_witness_inner(
			&DepositWitness {
				deposit_address: address.clone(),
				asset: btc::Asset::Btc,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details
			},
			Default::default()
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::BitcoinIngressEgress(Event::DepositFailed {
				details: DepositFailedDetails::DepositChannel {
					deposit_witness: DepositWitness {
						deposit_address: _,
						asset: btc::Asset::Btc,
						amount: DEFAULT_DEPOSIT_AMOUNT,
						deposit_details: _,
					},
				},
				reason: DepositFailedReason::TransactionRejectedByBroker,
				block_height: _,
			})
		);

		assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
	});
}

#[test]
fn marked_transactions_expire_if_not_witnessed() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_id);
		let expiry_at = System::block_number() + MARKED_TX_EXPIRATION_BLOCKS as u64;

		let (_, address) =
			helpers::request_address_and_deposit(BROKER, btc::Asset::Btc, deposit_details);
		let _ = DepositChannelLookup::<Test, Instance2>::get(address).unwrap();

		assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			tx_id,
		));

		System::set_block_number(expiry_at);

		BitcoinIngressEgress::on_idle(expiry_at, Weight::MAX);

		assert!(!TransactionsMarkedForRejection::<Test, Instance2>::contains_key(BROKER, tx_id));

		assert_has_event::<Test>(RuntimeEvent::BitcoinIngressEgress(
			Event::TransactionRejectionRequestExpired { account_id: BROKER, tx_id },
		));
	});
}

#[test]
fn only_broker_can_mark_transaction_for_rejection() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			BitcoinIngressEgress::mark_transaction_for_rejection(
				OriginTrait::signed(ALICE),
				Default::default(),
			),
			BadOrigin
		);

		assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			Default::default(),
		));
	});
}

#[test]
fn do_not_expire_marked_transactions_if_prewitnessed() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let expiry_at = System::block_number() + MARKED_TX_EXPIRATION_BLOCKS as u64;

		TransactionsMarkedForRejection::<Test, Instance2>::insert(
			BROKER,
			tx_id,
			TransactionRejectionStatus { prewitnessed: true, expires_at: u64::MAX },
		);

		ReportExpiresAt::<Test, Instance2>::insert(expiry_at, vec![(BROKER, tx_id)]);

		BitcoinIngressEgress::on_idle(expiry_at, Weight::MAX);

		assert!(TransactionsMarkedForRejection::<Test, Instance2>::contains_key(BROKER, tx_id));
	});
}

#[test]
fn can_not_report_transaction_after_witnessing() {
	new_test_ext().execute_with(|| {
		let unreported = Hash::random();
		let unseen = Hash::random();
		let prewitnessed = Hash::random();

		TransactionsMarkedForRejection::<Test, Instance2>::insert(
			BROKER,
			unseen,
			TransactionRejectionStatus { prewitnessed: false, expires_at: u64::MAX },
		);
		TransactionsMarkedForRejection::<Test, Instance2>::insert(
			BROKER,
			prewitnessed,
			TransactionRejectionStatus { prewitnessed: true, expires_at: u64::MAX },
		);

		assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			unreported,
		));
		assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			unseen,
		));
		assert_noop!(
			BitcoinIngressEgress::mark_transaction_for_rejection(
				OriginTrait::signed(BROKER),
				prewitnessed,
			),
			crate::Error::<Test, Instance2>::TransactionAlreadyPrewitnessed
		);
	});
}

#[test]
fn send_funds_back_after_they_have_been_rejected() {
	new_test_ext().execute_with(|| {
		let deposit_details = helpers::generate_btc_deposit(Hash::random());

		assert_ok!(crate::Pallet::<Test, Instance2>::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			deposit_details.id.tx_id
		));

		helpers::request_address_and_deposit(BROKER, btc::Asset::Btc, deposit_details.clone());

		assert_eq!(MockEgressBroadcasterBtc::get_pending_api_calls().len(), 0);
		assert_eq!(ScheduledTransactionsForRejection::<Test, Instance2>::get().len(), 1);

		BitcoinIngressEgress::on_finalize(1);

		assert_eq!(ScheduledTransactionsForRejection::<Test, Instance2>::get().len(), 0);

		assert_has_matching_event!(
			Test,
			RuntimeEvent::BitcoinIngressEgress(Event::TransactionRejectedByBroker {
				broadcast_id: _,
				tx_id: _,
			})
		);

		assert_eq!(
			MockEgressBroadcasterBtc::get_pending_api_calls().len(),
			1,
			"Expected 1 call, got: {:#?}, events: {:#?}",
			MockEgressBroadcasterBtc::get_pending_api_calls(),
			System::events(),
		);
	});
}

#[test]
fn test_mark_transaction_expiry_and_deposit() {
	let tx_id = Hash::random();

	let ext = new_test_ext()
		// Mark a transaction
		.then_apply_extrinsics(|_| {
			[(
				OriginTrait::signed(BROKER),
				crate::Call::<Test, Instance2>::mark_transaction_for_rejection { tx_id },
				Ok(()),
			)]
		})
		// Advance 10 blocks
		.then_process_blocks(10)
		// Mark the same transaction again
		.then_apply_extrinsics(|_| {
			[(
				OriginTrait::signed(BROKER),
				crate::Call::<Test, Instance2>::mark_transaction_for_rejection { tx_id },
				Ok(()),
			)]
		})
		// Get expiry block of the first report
		.then_execute_with(|_| {
			let mut expiries = ReportExpiresAt::<Test, Instance2>::iter().collect::<Vec<_>>();
			expiries.sort_by_key(|(block, _)| *block);
			(expiries[0].0, expiries[1].0)
		});
	let (first_expiry, second_expiry) = *ext.context();

	ext
		// Advance to the block after expiry block
		.then_execute_at_block(first_expiry, |_| {
			// First expiry should be triggered, but ignored.
		})
		.then_process_events(|_, event| {
			if let RuntimeEvent::BitcoinIngressEgress(Event::TransactionRejectionRequestExpired {
				..
			}) = event
			{
				panic!("Rejection Request Expired prematurely");
			}
			None::<()>
		})
		.then_execute_at_block(second_expiry, |_| {
			// Second expiry should be triggered, expiry is processed.
		})
		.then_execute_with_keep_context(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::BitcoinIngressEgress(Event::TransactionRejectionRequestExpired {
					account_id: BROKER,
					..
				})
			);
		});
}

#[test]
fn can_report_between_prewitness_and_witness_if_tx_was_not_boosted() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_id);

		let (_id, address, ..) = BitcoinIngressEgress::request_liquidity_deposit_address(
			BROKER,
			btc::Asset::Btc,
			0,
			ForeignChainAddress::Btc(ScriptPubkey::P2SH(DEFAULT_BTC_ADDRESS)),
		)
		.unwrap();

		let deposit_address = match address {
			ForeignChainAddress::Btc(script_pubkey) => script_pubkey,
			_ => unreachable!(),
		};

		let deposit_witness = DepositWitness {
			deposit_address,
			asset: btc::Asset::Btc,
			amount: DEFAULT_DEPOSIT_AMOUNT,
			deposit_details,
		};

		assert_ok!(BitcoinIngressEgress::process_channel_deposit_prewitness(
			deposit_witness.clone(),
			10,
		));
		assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			tx_id
		));
		assert_ok!(BitcoinIngressEgress::process_channel_deposit_full_witness_inner(
			&deposit_witness,
			10
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::BitcoinIngressEgress(Event::DepositFailed {
				details: DepositFailedDetails::DepositChannel {
					deposit_witness: DepositWitness {
						deposit_address: _,
						asset: btc::Asset::Btc,
						amount: DEFAULT_DEPOSIT_AMOUNT,
						deposit_details: _,
					},
				},
				reason: DepositFailedReason::TransactionRejectedByBroker,
				block_height: _,
			})
		);

		assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
	});
}

#[test]
fn gets_rejected_if_vault_transaction_was_aborted_and_rejected() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_id);

		let vault_swap = VaultDepositWitness::<Test, Instance2> {
			input_asset: Asset::Btc.try_into().unwrap(),
			deposit_address: Default::default(),
			channel_id: Some(0),
			deposit_amount: 100,
			deposit_details,
			output_asset: Asset::Eth,
			destination_address: EncodedAddress::Eth(Default::default()),
			deposit_metadata: Default::default(),
			tx_id,
			broker_fee: Some(Beneficiary { account: BROKER, bps: 0 }),
			affiliate_fees: Default::default(),
			refund_params: ChannelRefundParametersForChain::<Bitcoin> {
				retry_duration: 0,
				min_price: U256::from(0),
				refund_address: ScriptPubkey::P2SH(DEFAULT_BTC_ADDRESS),
				refund_ccm_metadata: None,
				max_oracle_price_slippage: None,
			},
			dca_params: None,
			boost_fee: 0,
		};

		assert_ok!(BitcoinIngressEgress::mark_transaction_for_rejection(
			OriginTrait::signed(BROKER),
			tx_id,
		));

		BitcoinIngressEgress::process_vault_swap_request_prewitness(0, vault_swap.clone());

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance2>::get().len(),
			0,
			"Refund broadcast should not have been scheduled!"
		);

		BitcoinIngressEgress::process_vault_swap_request_full_witness(0, vault_swap);

		assert!(
			MockSwapRequestHandler::<Test>::get_swap_requests().is_empty(),
			"No swaps should have been triggered!"
		);

		assert_eq!(
			ScheduledEgressFetchOrTransfer::<Test, Instance2>::get().len(),
			0,
			"Refund broadcast should not have been scheduled!"
		);

		assert_eq!(ScheduledTransactionsForRejection::<Test, Instance2>::decode_len(), Some(1));
	});
}
