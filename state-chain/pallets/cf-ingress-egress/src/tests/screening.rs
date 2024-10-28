use crate::mock_btc::*;

use cf_chains::btc::ScriptPubkey;

use cf_traits::mocks::account_role_registry::MockAccountRoleRegistry;

use cf_primitives::{Beneficiaries, ChannelId};

use cf_traits::DepositApi;

use crate::DepositWitness;

use cf_chains::btc::Hash;

use sp_runtime::DispatchError::BadOrigin;

use crate::TaintedTransactions;

use crate::TaintedTransactionDetails;

use cf_traits::BalanceApi;

use cf_traits::AccountRoleRegistry;

use crate::ReportExpiresAt;

use crate::TaintedTransactionStatus;

use cf_chains::ForeignChainAddress;

use crate::TAINTED_TX_EXPIRATION_BLOCKS;

use cf_chains::btc::deposit_address::DepositAddress;

use cf_chains::btc::BtcDepositDetails;

use cf_primitives::chains::assets::btc;

use crate::tests::ALICE;

use crate::BoostPoolId;

use cf_chains::btc::UtxoId;

const DEFAULT_DEPOSIT_AMOUNT: u64 = 1_000;

use crate::DepositIgnoredReason;
use frame_support::{
	assert_noop, assert_ok,
	traits::{Hooks, OriginTrait},
	weights::Weight,
};

use cf_test_utilities::{assert_has_event, assert_has_matching_event};

use crate::tests::BROKER;

use crate::{DepositChannelLookup, ScheduledTxForReject};

const DEFAULT_BTC_ADDRESS: [u8; 20] = [0; 20];

mod helpers {

	use super::*;

	pub fn generate_address(bytes: [u8; 20]) -> ForeignChainAddress {
		ForeignChainAddress::Btc(ScriptPubkey::P2SH(bytes))
	}

	pub fn generate_btc_deposit(tx_in_id: Hash) -> BtcDepositDetails {
		BtcDepositDetails {
			utxo_id: UtxoId { tx_id: tx_in_id, vout: 0 },
			deposit_address: DepositAddress { pubkey_x: [0; 32], script_path: None },
		}
	}

	pub fn request_address_and_deposit(
		who: ChannelId,
		asset: btc::Asset,
		deposit_details: BtcDepositDetails,
	) -> (ChannelId, <Bitcoin as Chain>::ChainAccount) {
		let (id, address, ..) = IngressEgress::request_liquidity_deposit_address(
			who,
			asset,
			0,
			generate_address(DEFAULT_BTC_ADDRESS),
		)
		.unwrap();
		let address: <Bitcoin as Chain>::ChainAccount = address.try_into().unwrap();
		assert_ok!(IngressEgress::process_single_deposit(
			address.clone(),
			asset,
			DEFAULT_DEPOSIT_AMOUNT,
			deposit_details,
			Default::default()
		));
		(id, address)
	}

	pub fn setup_boost_swap() -> ForeignChainAddress {
		assert_ok!(
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
				&ALICE,
			)
		);

		assert_ok!(IngressEgress::create_boost_pools(
			RuntimeOrigin::root(),
			vec![BoostPoolId { asset: btc::Asset::Btc, tier: 10 }],
		));

		<Test as crate::Config>::Balance::try_credit_account(&ALICE, btc::Asset::Btc.into(), 1000)
			.unwrap();

		let (_, address, _, _) = IngressEgress::request_swap_deposit_address(
			btc::Asset::Btc,
			btc::Asset::Btc.into(),
			generate_address(DEFAULT_BTC_ADDRESS),
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
			btc::Asset::Btc,
			1000,
			10
		));

		address
	}
}

#[test]
fn process_tainted_transaction_and_expect_refund() {
	new_test_ext().execute_with(|| {
		let tx_in_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_in_id);
		let (_, address) =
			helpers::request_address_and_deposit(BROKER, btc::Asset::Btc, deposit_details.clone());
		let _ = DepositChannelLookup::<Test, ()>::get(address.clone()).unwrap();

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));

		assert_ok!(IngressEgress::mark_transaction_as_tainted(
			OriginTrait::signed(BROKER),
			tx_in_id,
		));

		assert_ok!(IngressEgress::process_single_deposit(
			address,
			btc::Asset::Btc,
			DEFAULT_DEPOSIT_AMOUNT,
			deposit_details,
			Default::default()
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::<Test, ()>::DepositIgnored {
				deposit_address: _address,
				asset: btc::Asset::Btc,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: _,
				reason: DepositIgnoredReason::TransactionTainted,
			})
		);

		assert_eq!(ScheduledTxForReject::<Test, ()>::decode_len(), Some(1));
	});
}

#[test]
fn finalize_boosted_tx_if_tainted_after_prewitness() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_id);

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));

		let address: <Bitcoin as Chain>::ChainAccount =
			helpers::setup_boost_swap().try_into().unwrap();

		let _ = IngressEgress::add_prewitnessed_deposits(
			vec![DepositWitness {
				deposit_address: address.clone(),
				asset: btc::Asset::Btc,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: deposit_details.clone(),
			}],
			10,
		);

		assert_noop!(
			IngressEgress::mark_transaction_as_tainted(OriginTrait::signed(BROKER), tx_id,),
			crate::Error::<Test, ()>::TransactionAlreadyPrewitnessed
		);

		assert_ok!(IngressEgress::process_single_deposit(
			address,
			btc::Asset::Btc,
			DEFAULT_DEPOSIT_AMOUNT,
			deposit_details,
			Default::default()
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::DepositFinalised {
				deposit_address: _,
				asset: btc::Asset::Btc,
				..
			})
		);
	});
}

#[test]
fn reject_tx_if_tainted_before_prewitness() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_id);

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));

		let address: <Bitcoin as Chain>::ChainAccount =
			helpers::setup_boost_swap().try_into().unwrap();

		assert_ok!(IngressEgress::mark_transaction_as_tainted(OriginTrait::signed(BROKER), tx_id,));

		let _ = IngressEgress::add_prewitnessed_deposits(
			vec![DepositWitness {
				deposit_address: address.clone(),
				asset: btc::Asset::Btc,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: deposit_details.clone(),
			}],
			10,
		);

		assert_ok!(IngressEgress::process_single_deposit(
			address,
			btc::Asset::Btc,
			DEFAULT_DEPOSIT_AMOUNT,
			deposit_details,
			Default::default()
		));

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::DepositIgnored {
				deposit_address: _,
				asset: btc::Asset::Btc,
				amount: DEFAULT_DEPOSIT_AMOUNT,
				deposit_details: _,
				reason: DepositIgnoredReason::TransactionTainted,
			})
		);
	});
}

#[test]
fn tainted_transactions_expire_if_not_witnessed() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let deposit_details = helpers::generate_btc_deposit(tx_id);
		let expiry_at = System::block_number() + TAINTED_TX_EXPIRATION_BLOCKS as u64;

		let (_, address) =
			helpers::request_address_and_deposit(BROKER, btc::Asset::Btc, deposit_details);
		let _ = DepositChannelLookup::<Test, ()>::get(address).unwrap();

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(
			&BROKER,
		));

		assert_ok!(IngressEgress::mark_transaction_as_tainted(OriginTrait::signed(BROKER), tx_id,));

		System::set_block_number(expiry_at);

		IngressEgress::on_idle(expiry_at, Weight::MAX);

		assert!(!TaintedTransactions::<Test, ()>::contains_key(BROKER, tx_id));

		assert_has_event::<Test>(RuntimeEvent::IngressEgress(
			crate::Event::TaintedTransactionReportExpired { account_id: BROKER, tx_id },
		));
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
fn do_not_expire_tainted_transactions_if_prewitnessed() {
	new_test_ext().execute_with(|| {
		let tx_id = Hash::random();
		let expiry_at = System::block_number() + TAINTED_TX_EXPIRATION_BLOCKS as u64;

		TaintedTransactions::<Test, ()>::insert(
			BROKER,
			tx_id,
			TaintedTransactionStatus::Prewitnessed,
		);

		ReportExpiresAt::<Test, ()>::insert(expiry_at, vec![(BROKER, tx_id)]);

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

		let unreported = Hash::random();
		let unseen = Hash::random();
		let prewitnessed = Hash::random();
		let boosted = Hash::random();

		TaintedTransactions::<Test, ()>::insert(BROKER, unseen, TaintedTransactionStatus::Unseen);
		TaintedTransactions::<Test, ()>::insert(
			BROKER,
			prewitnessed,
			TaintedTransactionStatus::Prewitnessed,
		);
		TaintedTransactions::<Test, ()>::insert(BROKER, boosted, TaintedTransactionStatus::Boosted);

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

#[test]
fn send_funds_back_after_they_have_been_rejected() {
	new_test_ext().execute_with(|| {
		let deposit_details = helpers::generate_btc_deposit(Hash::random());
		let tainted_tx_details = TaintedTransactionDetails {
			refund_address: Some(helpers::generate_address(DEFAULT_BTC_ADDRESS)),
			amount: DEFAULT_DEPOSIT_AMOUNT,
			asset: btc::Asset::Btc,
			deposit_details,
		};

		ScheduledTxForReject::<Test, ()>::append(tainted_tx_details);

		IngressEgress::on_finalize(1);

		assert_eq!(ScheduledTxForReject::<Test, ()>::decode_len(), None);

		assert_has_matching_event!(
			Test,
			RuntimeEvent::IngressEgress(crate::Event::TaintedTransactionRejected {
				broadcast_id: _,
				tx_id: _,
			})
		);
	});
}
