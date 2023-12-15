use std::collections::HashMap;

use cf_primitives::EpochIndex;
use futures_core::Future;
use itertools::Itertools;
use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness};
use secp256k1::hashes::Hash as secp256k1Hash;
use state_chain_runtime::BitcoinInstance;

use super::super::common::chunked_chain_source::chunked_by_vault::{
	builder::ChunkedByVaultBuilder, ChunkedByVault,
};
use crate::{
	btc::rpc::VerboseTransaction,
	witness::common::{
		chunked_chain_source::chunked_by_vault::deposit_addresses::Addresses, RuntimeCallHasChain,
		RuntimeHasChain,
	},
};
use bitcoin::BlockHash;
use cf_chains::{
	assets::btc,
	btc::{ScriptPubkey, UtxoId},
	Bitcoin,
};

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn btc_deposits<ProcessCall, ProcessingFut>(
		self,
		process_call: ProcessCall,
	) -> ChunkedByVaultBuilder<
		impl ChunkedByVault<
			Index = u64,
			Hash = BlockHash,
			Data = Vec<VerboseTransaction>,
			Chain = Bitcoin,
		>,
	>
	where
		Inner: ChunkedByVault<
			Index = u64,
			Hash = BlockHash,
			Data = (((), Vec<VerboseTransaction>), Addresses<Inner>),
			Chain = Bitcoin,
		>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		self.then(move |epoch, header| {
			let process_call = process_call.clone();
			async move {
				// TODO: Make addresses a Map of some kind?
				let (((), txs), addresses) = header.data;

				let script_addresses = script_addresses(addresses);

				let deposit_witnesses = deposit_witnesses(&txs, script_addresses);

				// Submit all deposit witnesses for the block.
				if !deposit_witnesses.is_empty() {
					process_call(
						pallet_cf_ingress_egress::Call::<_, BitcoinInstance>::process_deposits {
							deposit_witnesses,
							block_height: header.index,
						}
						.into(),
						epoch.index,
					)
					.await;
				}
				txs
			}
		})
	}
}

fn deposit_witnesses(
	txs: &Vec<VerboseTransaction>,
	script_addresses: HashMap<Vec<u8>, ScriptPubkey>,
) -> Vec<DepositWitness<Bitcoin>> {
	let mut deposit_witnesses = Vec::new();
	for tx in txs {
		let tx_hash = tx.txid.as_raw_hash().to_byte_array();

		let deposits_in_tx = (0..)
			.zip(&tx.vout)
			.filter(|(_vout, tx_out)| tx_out.value.to_sat() > 0)
			.filter_map(|(vout, tx_out)| {
				let tx_script_pubkey_bytes = tx_out.script_pubkey.to_bytes();
				script_addresses.get(&tx_script_pubkey_bytes).map(
					|bitcoin_script| DepositWitness::<Bitcoin> {
						deposit_address: bitcoin_script.clone(),
						asset: btc::Asset::Btc,
						amount: tx_out.value.to_sat(),
						deposit_details: UtxoId { tx_id: tx_hash, vout },
					},
				)
			})
			// convert to bytes so it's Hashable
			.into_grouping_map_by(|d| d.deposit_address.bytes())
			// We only take the largest output of a tx as a deposit witness. This is to avoid
			// attackers spamming us with many small outputs in a tx. Inputs are more expensive than
			// outputs - thus, the attacker could send many outputs (cheap for them) which results
			// in us needing to sign many *inputs*, expensive for us. sort by descending by amount
			.max_by_key(|_address, d| d.amount);

		for (_script_pubkey, deposit) in deposits_in_tx {
			deposit_witnesses.push(deposit);
		}
	}
	deposit_witnesses
}

fn script_addresses(
	addresses: Vec<DepositChannelDetails<state_chain_runtime::Runtime, BitcoinInstance>>,
) -> HashMap<Vec<u8>, ScriptPubkey> {
	addresses
		.into_iter()
		.map(|channel| {
			assert_eq!(channel.deposit_channel.asset, btc::Asset::Btc);
			let script_pubkey = channel.deposit_channel.address;
			(script_pubkey.bytes(), script_pubkey)
		})
		.collect()
}

#[cfg(test)]
pub mod tests {

	use crate::btc::rpc::VerboseTxOut;

	use super::*;
	use bitcoin::{
		absolute::{Height, LockTime},
		block::Version,
		Amount, ScriptBuf, Txid,
	};
	use cf_chains::{
		btc::{deposit_address::DepositAddress, ScriptPubkey},
		DepositChannel,
	};
	use pallet_cf_ingress_egress::ChannelAction;
	use rand::Rng;
	use sp_runtime::AccountId32;

	pub fn fake_transaction(tx_outs: Vec<VerboseTxOut>, fee: Option<Amount>) -> VerboseTransaction {
		let random_bytes: [u8; 32] = rand::thread_rng().gen();
		let txid = Txid::from_byte_array(random_bytes);
		VerboseTransaction {
			txid,
			version: Version::from_consensus(2),
			locktime: LockTime::Blocks(Height::from_consensus(0).unwrap()),
			vin: vec![],
			vout: tx_outs,
			fee,
			// not important, we just need to set it to a value.
			hash: txid,
			size: Default::default(),
			vsize: Default::default(),
			weight: Default::default(),
			hex: Default::default(),
		}
	}

	fn fake_details(
		address: ScriptPubkey,
	) -> DepositChannelDetails<state_chain_runtime::Runtime, BitcoinInstance> {
		DepositChannelDetails::<_, BitcoinInstance> {
			opened_at: 1,
			expires_at: 10,
			deposit_channel: DepositChannel {
				channel_id: 1,
				address,
				asset: btc::Asset::Btc,
				state: DepositAddress::new([0; 32], 1),
			},
			action: ChannelAction::<AccountId32>::LiquidityProvision {
				lp_account: AccountId32::new([0xab; 32]),
			},
		}
	}

	pub fn fake_verbose_vouts(vals_and_scripts: Vec<(u64, Vec<u8>)>) -> Vec<VerboseTxOut> {
		vals_and_scripts
			.into_iter()
			.enumerate()
			.map(|(n, (value, script_bytes))| VerboseTxOut {
				value: Amount::from_sat(value),
				n: n as u64,
				script_pubkey: ScriptBuf::from(script_bytes),
			})
			.collect()
	}

	#[test]
	fn deposit_witnesses_no_utxos_no_monitored() {
		let txs = vec![fake_transaction(vec![], None), fake_transaction(vec![], None)];
		let deposit_witnesses = deposit_witnesses(&txs, HashMap::new());
		assert!(deposit_witnesses.is_empty());
	}

	#[test]
	fn filter_out_value_0() {
		let btc_deposit_script: ScriptPubkey = DepositAddress::new([0; 32], 9).script_pubkey();

		const UTXO_WITNESSED_1: u64 = 2324;
		let txs = vec![fake_transaction(
			fake_verbose_vouts(vec![
				(2324, btc_deposit_script.bytes()),
				(0, btc_deposit_script.bytes()),
			]),
			None,
		)];

		let deposit_witnesses =
			deposit_witnesses(&txs, script_addresses(vec![(fake_details(btc_deposit_script))]));
		assert_eq!(deposit_witnesses.len(), 1);
		assert_eq!(deposit_witnesses[0].amount, UTXO_WITNESSED_1);
	}

	#[test]
	fn deposit_witnesses_several_same_tx() {
		const LARGEST_UTXO_TO_DEPOSIT: u64 = 2324;
		const UTXO_TO_DEPOSIT_2: u64 = 1234;
		const UTXO_TO_DEPOSIT_3: u64 = 2000;

		let btc_deposit_script: ScriptPubkey = DepositAddress::new([0; 32], 9).script_pubkey();

		let txs = vec![
			fake_transaction(
				fake_verbose_vouts(vec![
					(UTXO_TO_DEPOSIT_2, btc_deposit_script.bytes()),
					(12223, vec![0, 32, 121, 9]),
					(LARGEST_UTXO_TO_DEPOSIT, btc_deposit_script.bytes()),
					(UTXO_TO_DEPOSIT_3, btc_deposit_script.bytes()),
				]),
				None,
			),
			fake_transaction(vec![], None),
		];

		let deposit_witnesses =
			deposit_witnesses(&txs, script_addresses(vec![fake_details(btc_deposit_script)]));
		assert_eq!(deposit_witnesses.len(), 1);
		assert_eq!(deposit_witnesses[0].amount, LARGEST_UTXO_TO_DEPOSIT);
	}

	#[test]
	fn deposit_witnesses_to_different_deposit_addresses_same_tx_is_witnessed() {
		const LARGEST_UTXO_TO_DEPOSIT: u64 = 2324;
		const UTXO_TO_DEPOSIT_2: u64 = 1234;
		const UTXO_FOR_SECOND_DEPOSIT: u64 = 2000;

		let btc_deposit_script_1: ScriptPubkey = DepositAddress::new([0; 32], 9).script_pubkey();
		let btc_deposit_script_2: ScriptPubkey = DepositAddress::new([0; 32], 1232).script_pubkey();

		let txs = vec![
			fake_transaction(
				fake_verbose_vouts(vec![
					(UTXO_TO_DEPOSIT_2, btc_deposit_script_1.bytes()),
					(12223, vec![0, 32, 121, 9]),
					(LARGEST_UTXO_TO_DEPOSIT, btc_deposit_script_1.bytes()),
					(UTXO_FOR_SECOND_DEPOSIT, btc_deposit_script_2.bytes()),
				]),
				None,
			),
			fake_transaction(vec![], None),
		];

		let mut deposit_witnesses = deposit_witnesses(
			&txs,
			// watching 2 addresses
			script_addresses(vec![
				fake_details(btc_deposit_script_1.clone()),
				fake_details(btc_deposit_script_2.clone()),
			]),
		);

		deposit_witnesses.sort_by_key(|d| d.deposit_address.clone());
		// We should have one deposit per address.
		assert_eq!(deposit_witnesses.len(), 2);
		assert_eq!(deposit_witnesses[0].amount, UTXO_FOR_SECOND_DEPOSIT);
		assert_eq!(deposit_witnesses[0].deposit_address, btc_deposit_script_2);
		assert_eq!(deposit_witnesses[1].amount, LARGEST_UTXO_TO_DEPOSIT);
		assert_eq!(deposit_witnesses[1].deposit_address, btc_deposit_script_1);
	}

	#[test]
	fn deposit_witnesses_several_diff_tx() {
		let btc_deposit_script: ScriptPubkey = DepositAddress::new([0; 32], 9).script_pubkey();

		const UTXO_WITNESSED_1: u64 = 2324;
		const UTXO_WITNESSED_2: u64 = 1234;
		let txs = vec![
			fake_transaction(
				fake_verbose_vouts(vec![
					(UTXO_WITNESSED_1, btc_deposit_script.bytes()),
					(12223, vec![0, 32, 121, 9]),
					(UTXO_WITNESSED_1 - 1, btc_deposit_script.bytes()),
				]),
				None,
			),
			fake_transaction(
				fake_verbose_vouts(vec![
					(UTXO_WITNESSED_2 - 10, btc_deposit_script.bytes()),
					(UTXO_WITNESSED_2, btc_deposit_script.bytes()),
				]),
				None,
			),
		];

		let deposit_witnesses =
			deposit_witnesses(&txs, script_addresses(vec![fake_details(btc_deposit_script)]));
		assert_eq!(deposit_witnesses.len(), 2);
		assert_eq!(deposit_witnesses[0].amount, UTXO_WITNESSED_1);
		assert_eq!(deposit_witnesses[1].amount, UTXO_WITNESSED_2);
	}
}
