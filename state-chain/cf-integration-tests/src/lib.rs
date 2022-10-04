#![cfg(test)]

mod network;

mod signer_nomination;

mod mock_runtime;

mod authorities;
mod genesis;
mod new_epoch;
mod staking;

use frame_support::{assert_noop, assert_ok, traits::OnInitialize};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::crypto::{Pair, Public};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use state_chain_runtime::{
	constants::common::*, opaque::SessionKeys, AccountId, Emissions, Flip, Governance, Origin,
	Reputation, Runtime, Staking, System, Timestamp, Validator, Witnesser,
};

use cf_primitives::{AuthorityCount, EpochIndex};
use cf_traits::{BlockNumber, FlipBalance};
use libsecp256k1::SecretKey;
use pallet_cf_staking::{EthTransactionHash, EthereumAddress};
use rand::{prelude::*, SeedableRng};
use sp_runtime::AccountId32;

type NodeId = AccountId32;
const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
const TX_HASH: EthTransactionHash = [211u8; 32];

pub const GENESIS_KEY: u64 = 42;

// TODO - remove collision of account numbers
pub const ALICE: [u8; 32] = [0xaa; 32];
pub const BOB: [u8; 32] = [0xbb; 32];
pub const CHARLIE: [u8; 32] = [0xcc; 32];
// Root and Gov member
pub const ERIN: [u8; 32] = [0xee; 32];

const GENESIS_EPOCH: EpochIndex = 1;

pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

// The minimum number of blocks a vault rotation should last
const VAULT_ROTATION_BLOCKS: BlockNumber = 6;

mod runtime {
	use super::*;
	use frame_support::dispatch::GetDispatchInfo;
	use pallet_cf_flip::FlipTransactionPayment;
	use pallet_transaction_payment::OnChargeTransaction;

	#[test]
	// We have two types of accounts. One set of accounts which is part
	// of the governance and is allowed to make free calls to governance extrinsic.
	// All other accounts are normally charged and can call any extrinsic.
	fn restriction_handling() {
		super::genesis::default().build().execute_with(|| {
			let call: state_chain_runtime::Call =
				frame_system::Call::remark { remark: vec![] }.into();
			let gov_call: state_chain_runtime::Call =
				pallet_cf_governance::Call::approve { id: 1 }.into();
			// Expect a successful normal call to work
			let ordinary = FlipTransactionPayment::<Runtime>::withdraw_fee(
				&ALICE.into(),
				&call,
				&call.get_dispatch_info(),
				5,
				0,
			);
			assert!(ordinary.expect("we have a result").is_some(), "expected Some(Surplus)");
			// Expect a successful gov call to work
			let gov = FlipTransactionPayment::<Runtime>::withdraw_fee(
				&ERIN.into(),
				&gov_call,
				&gov_call.get_dispatch_info(),
				5000,
				0,
			);
			assert!(gov.expect("we have a result").is_none(), "expected None");
			// Expect a non gov call to fail when it's executed by gov member
			let gov_err = FlipTransactionPayment::<Runtime>::withdraw_fee(
				&ERIN.into(),
				&call,
				&call.get_dispatch_info(),
				5000,
				0,
			);
			assert!(gov_err.is_err(), "expected an error");
		});
	}
}
